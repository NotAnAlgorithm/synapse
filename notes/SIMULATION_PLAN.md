# Simulation & Efficacy Testing Plan — Synapse

**How to test Synapse's learning/retention algorithms at small scale, with no months of prep and no real students — by simulating students correctly.**

> Status: plan (proposal). The **memory/roadmap claims and code citations here
> were checked against this checkout** (file:symbol inline); everything under
> "the method" and "the scoped build" is *design*, not yet implemented. Companion
> reading: `notes/prd.md` (the one goal + kill-criteria discipline),
> `notes/ARCHITECTURE.md` (core/RPC layering), and the roadmap
> (`~/.claude/plans/velvet-brewing-cookie.md`). Where this doc says "verified,"
> it means read in-tree on 2026-07-06.

---

## 1. The question this answers

Synapse's single objective is **score gain per study hour** (PRD §1). We have shipped
seven retention-relevant mechanisms on top of the Anki/FSRS core and want to know —
*before* committing scarce real-student time — which of them actually earn their
keep, and with what settings. We have no months of runway and no cohort of students,
so students must be **simulated**.

The mechanisms under test (all verified in-tree or in the roadmap):

| Mechanism | Where it lives | What it's supposed to buy |
|---|---|---|
| FSRS @ 0.9 desired retention | `fsrs-rs` crate + `scheduler/fsrs/` | efficient steady-state retention |
| Test-date governor | `scheduler/governor.rs` (`governor_effective_retention`) | recall *peaking on exam day* |
| Concept + type interleaving | `scheduler/queue/builder/` (`interleave.rs`, flag `interleave_by_concept` #47) | durable discrimination between confusable concepts |
| Mastery gating | queue-build filter (flag `mastery_gating` #48) + `concept_edges` | stop wasting reviews on unlearnable-yet cards |
| Trickle-down credit | answer path (flag `trickle_down_credit` #49) | fewer reviews via prereq credit |
| Add-then-fade metamorphosis | `scheduler/metamorphosis.rs` (`apply_metamorphosis_after_answer`, flag #50) | remove workload without losing retention |
| Error-driven minting | `qt/aqt/synapse/mint.py` + `reviewer_did_answer_card` | close the *specific* gap a miss exposed |

---

## 2. The one thing that decides whether any of this is valid

Generating simulated students is trivial. Generating them so the verdict is
**trustworthy** is the whole problem, and it has one failure mode:

**Circularity (model–scheduler congruence).** Our existing simulator draws a
synthetic learner from the FSRS DSR model (difficulty/stability/retrievability,
power-law forgetting) and then schedules that learner with FSRS. FSRS will look
optimal — because its own assumptions *are* the ground truth. Worse: **any mechanism
FSRS does not model will look worthless or harmful** in that simulator.

This is not hypothetical for us. The stock engine is a **closed FSRS-vs-FSRS loop**:
`Collection::simulate_review` / `simulate_workload`
(`rslib/src/scheduler/fsrs/simulator.rs:249,277`) build a config and hand the entire
forward simulation to `fsrs::simulate()` (`simulator.rs:254`) inside the external
`fsrs-rs` crate. That loop:

- samples recall from FSRS's own retrievability curve;
- schedules with FSRS intervals;
- has **no concept graph, no prerequisites, no interference term** (cards are i.i.d.);
- never calls `answer_card`, the real queue builder, the governor, gating,
  trickle-down, or metamorphosis. The only policy knob it exposes is a
  `ReviewPriorityFn` *ordering* proxy (`simulator.rs:82`), not our real queue logic.

So the built-in simulator is the **right tool for the FSRS knobs and a
wrong-by-construction judge for our differentiators**:

| Mechanism | Visible to the stock `simulate_*`? |
|---|---|
| FSRS retention target, load balancer, easy-days | ✅ home turf (already wired: `apply_load_balance_and_easy_days`, `simulator.rs:35`) |
| Test-date governor | ⚠️ derivable (fixed exam date + measure at that date), but governor code isn't in the loop |
| Concept interleaving | ❌ needs a cross-item **interference** term DSR lacks |
| Mastery gating | ❌ needs a **prerequisite structure** — cards are independent |
| Trickle-down credit | ❌ needs **cross-concept coupling** |
| Minting / metamorphosis | ⚠️ needs a **gap→item causal** link + workload accounting |

**Conclusion that drives the rest of this doc:** to test the differentiators we must
build one *richer, deliberately non-FSRS* student model and drive our *real*
scheduler code with it. The stock simulator becomes phase 0, not the answer.

---

## 3. Reframe "small scale," and the two evaluation modes

**Small scale ≠ few simulated students.** We can run 10⁴ virtual learners overnight,
so **statistical power is free**. The binding constraints are (a) *model validity* and
(b) *few real humans to validate against*. Spend effort on a credible, multi-flavoured
student model — not on N.

Two modes do different jobs; conflating them is the classic error:

- **Offline replay / backtesting — validates the *memory model*.** Feed *real* review
  histories, score how well a model *predicts* recall. **We already own this**:
  `Collection::compute_params` / `evaluate_params` return log-loss + RMSE-over-bins,
  with a health-check band (adjusted log-loss ≤ 1.11 or RMSE ≤ 1.53)
  (`rslib/src/scheduler/fsrs/params.rs`, health check ~`:194`). This **cannot rank
  scheduling policies** — you only observe outcomes at the intervals the *original*
  scheduler chose (the counterfactual / off-policy problem). You never see what a
  different policy would have produced.
- **Forward Monte-Carlo simulation — compares *policies*.** With a *validated* memory
  model, run each policy forward, sample pass/fail, update latent memory, measure
  outcomes. This is the **only** way to compare interleaving-on vs -off, gating,
  the governor, etc.

We need both: replay to earn trust in the model, forward sim to rank the algorithms.
We can also export real logs to seed calibration without any of our own students, via
the existing `ExportDataset` RPC (`params.rs`, `export_dataset` ~`:225`).

---

## 4. The method (process, end to end)

### Step 0 — Define the outcome, tied to the north star
Primary output is an **efficiency frontier**, never a single number:
*simulated exam-day mastery vs. total study minutes (and total reviews)*. Every
comparison holds one axis fixed and reads the other ("the governor buys +X% exam-day
retention at *equal* total workload"). Secondary: retention-over-time curves,
reviews/day (spikes matter), time-to-mastery of downstream concepts, and
**calibration of our own displayed scores** (F2 Performance, F3 readiness) against
simulated true mastery — a direct test of the "honest metrics" promise.

### Step 1 — Build the ground-truth student model(s), deliberately *unlike* the scheduler
This is the load-bearing work.

1. **Use ≥3 distinct memory-model families as separate "truths"** and report the
   *range* of each effect across them. A mechanism that wins only under FSRS's own
   model is an artifact; one that wins across families is real. Candidates:
   - Power-law (FSRS's own) — include, never alone.
   - Exponential decay (classic Ebbinghaus).
   - **ACT-R declarative memory** (activation-based; produces spacing effects for free).
   - **Bjork New Theory of Disuse** (storage vs. retrieval strength) — the family that
     intrinsically expresses *desirable difficulty*, spacing, and why harder retrieval
     helps. If we want interleaving/minting to have *any* chance of showing benefit, at
     least one truth must be in this family.
   - Duolingo Half-Life Regression as a lightweight cross-check.
2. **Add the structure FSRS lacks**, or the differentiators are untestable:
   - **A "true" prerequisite DAG** over concepts, *authored separately* from the
     scheduler's `concept_edges`. Learnability of a concept is gated by mastery of its
     true prereqs. Now gating and trickle-down have something to exploit — or to fail
     against (feed a deliberately *wrong* edge and gating should visibly hurt).
   - **A confusability matrix** between concepts. Recall depends on discriminability
     from *recently studied* confusable items. Blocking inflates short-term recall but
     degrades discrimination; interleaving trades short- for long-term. Without this
     term, interleaving is pure noise.
   - **A gap→item causal link** for minting: a miss maps to a specific concept deficit;
     a well-minted card raises that concept faster than a random review. Parameterise
     "mint quality" so the whole loop can be stressed.

### Step 2 — Calibrate to real data, with *no* students of our own
Fit memory-model parameters *and their population distribution* to the **public FSRS
benchmark dataset** (Anki's open corpus; `fsrs-rs` ships a benchmark harness) via the
offline-replay tooling in Step 3's "mode 1," and confirm log-loss/RMSE land in the
sane band above. Now synthetic learners forget at realistic rates and spread.
`ExportDataset` lets us re-calibrate on Synapse-specific logs once any exist.

### Step 3 — Instantiate a heterogeneous population + content + behaviour model
- **Learners:** sample intrinsic parameters across the fitted distribution — fast/slow,
  high/low prior knowledge, different daily time budgets.
- **Content:** a concept set with the true prereq DAG, per-item difficulties, and the
  confusability structure; seed its *shape* from the real MCAT concept graph.
- **Behaviour / adherence:** humans don't review on schedule — model timing jitter,
  skipped days, backlog, session caps, and cramming. Adherence often dominates
  algorithm choice, so we want to *see* which mechanisms are robust to lapses.

### Step 4 — Drive the *real* Synapse code, not a re-implementation
Highest-fidelity move (scoped concretely in §6): the harness feeds simulated
pass/fail into the **actual** `Collection::answer_card` (`answering/mod.rs:311`) so the
real `answer_card_inner`, interleaving queue builder, `governor_effective_retention`,
`apply_metamorphosis_after_answer`, gating filter, and load balancer all execute. The
student model only decides recall; the *scheduler under test is shipped code*. A green
result then means the shipped algorithm works, not that a model of it works.

### Step 5 — Experimental design that kills variance
- **Within-subject / paired:** every virtual learner experiences every arm; compare on
  the *same* learner.
- **Common random numbers:** share the recall-draw RNG stream across arms so
  differences reflect the algorithm, not luck (often a 10× cut in required N).
- **Anchor arms:** always include a **baseline** (stock Anki SM-2 or flat FSRS) and an
  **oracle** (scheduler with access to true latent memory) to bound achievable range —
  an effect matters only relative to available headroom.
- **Ablations:** toggle mechanisms singly and in combination (gating alone,
  trickle-down alone, both) — interactions matter (gating + trickle-down can
  double-count credit).

### Step 6 — Analysis, sensitivity, pre-registered kill criteria
- Report effect **sizes with CIs**, framed as "under model M" — never as human truth.
- **Sensitivity-sweep** the memory-model parameters and structure; the deliverable is
  "interleaving helps 3–8% across plausible models and never hurts," not a point value.
- **Pre-register** the metric and kill thresholds before running; mirror the PRD's M3
  A/B kill criteria so the sim is a dry run of the real experiment.

### Step 7 — The honesty boundary
Simulation **ranks policies, sizes effects, and catches pathologies** under explicit
assumptions. It **cannot** prove the human magnitude of spacing / interleaving /
desirable-difficulty / minting benefits — those live in the psychology we *model*, not
*measure*. The endgame: sim filters seven mechanisms down to the two or three worth a
**tiny real pilot** (n = 5–20, within-subject on a content subset, a few weeks). Sim's
real payoff is telling us where to spend that scarce human time (§7 revisited below).

---

## 5. Per-mechanism: what "efficacy" means + the gotcha

- **Governor** — fix an exam date; measure recall *at that date* vs. flat FSRS **at
  equal total reviews**. Watch for a pre-exam **cram spike** in daily workload (raising
  DR toward 0.97 piles reviews); that's a real cost the frontier must show. The code
  already refuses to lower retention early (`governor.rs` "direction discipline"), so
  the sim should confirm the *up-only* ramp never hurts steady-state.
- **Interleaving** — only visible with the confusability term. Expect it to
  *underperform* blocking on short-horizon recall and *win* on delayed exam-day
  discrimination. No crossover ⇒ interference model too weak.
- **Mastery gating** — primary win: fewer wasted reviews on not-yet-learnable cards
  (faster downstream time-to-mastery). Primary risk: **starvation** (cards locked
  forever behind an unmastered prereq). Instrument queue-starvation explicitly; test a
  wrong prereq edge to confirm graceful degradation.
- **Trickle-down credit** — watch for **over-crediting**: prereqs marked mastered
  without direct evidence, corrupting gating. Compare true vs. credited mastery.
- **Minting + metamorphosis** — vary mint quality; measure concept-mastery gain per
  minted review vs. a random extra review. Metamorphosis efficacy = workload *removed*
  (faded siblings) with no retention loss on the underlying concept. Note metamorphosis
  fires at *mastery* and fading is reversible (`metamorphosis.rs` header) — the sim must
  let the governor re-surface faded cards before the exam.

---

## 6. Scoped build — the Synapse simulation harness (Step 4, concretely)

### 6.1 Shape and placement
A **dev-only workspace crate** (proposed `sim/`, a binary depending on the `anki`
crate) — kept out of the shipped wheel, invoked via a new `just sim` recipe (honouring
the "everything is a just recipe" rule in `CLAUDE.md`). Rust, because the entire code
under test is Rust and a 10⁴-learner × 180-day loop can't afford per-call FFI. It opens
Anki's **in-memory test collection** (`open_test_collection`, `rslib/src/tests.rs`,
used across `rslib` tests), seeds synthetic content directly in Rust, and drives the
real scheduler.

```
sim/
  Cargo.toml            # depends on anki (path), rand, rayon, serde, csv
  src/
    main.rs             # CLI: pick arms, N learners, horizon, seed, out dir
    student/            # the ground-truth models (the ONLY psychology)
      mod.rs            # StudentModel trait
      power_law.rs      # FSRS-congruent baseline truth
      exponential.rs    # Ebbinghaus
      actr.rs           # activation-based (spacing for free)
      disuse.rs         # storage/retrieval strength (desirable difficulty)
    world/
      content.rs        # concept DAG, item difficulties, confusability matrix
      population.rs      # learner-parameter sampling (fit to public dataset)
      behaviour.rs      # adherence: skips, jitter, backlog, session caps
    driver.rs           # the real-code loop (build_queues + answer_card)
    metrics.rs          # frontier, calibration, starvation, workload, CIs
    arms.rs             # baseline / oracle / feature-flag matrices
```

### 6.2 The one seam that keeps psychology out of the scheduler
```rust
/// Ground-truth latent memory of ONE simulated learner. This is the only place
/// psychological assumptions live; it is deliberately richer than — and
/// independent of — the FSRS DSR model the scheduler assumes.
pub trait StudentModel: Send {
    /// P(recall `item` right now), given full history. MAY depend on prerequisite
    /// mastery and on recently-seen confusable items — neither of which FSRS models.
    fn recall_prob(&self, item: ItemId, ctx: &ReviewContext) -> f64;

    /// Update latent state after a graded study event, INCLUDING spillover to
    /// prerequisite / confusable concepts.
    fn observe(&mut self, item: ItemId, grade: Grade, ctx: &ReviewContext);

    /// True underlying mastery of a concept in [0,1] — for SCORING ONLY.
    /// The scheduler never sees this (except the oracle arm).
    fn true_mastery(&self, concept: ConceptId) -> f64;
}
```

### 6.3 The driver loop — drives shipped code, model only decides recall
```
seed collection: Synapse deck + preset (flags per arm), synthetic notes with
                 concept::<section>::<id> tags, authored concept_edges
for day in 0..horizon:
    advance clock to `day`; per behaviour model, maybe skip / carry backlog
    col.build_queues()                       // REAL queue builder: interleaving + gating
    for card in due_queue[..session_cap]:
        item  = resolve(card)                // -> synthetic item + concept
        p     = student.recall_prob(item, ctx)
        grade = bernoulli(p, rng_shared_across_arms)     // common random numbers
        col.answer_card(&mut CardAnswer{ card_id, rating: grade, .. })
                                             // REAL scheduler + governor + metamorphosis
        student.observe(item, grade, ctx)
        if grade == Again && is_mcat_application(card):
            mint_linked_card(col, card)      // REAL minting seam
    metrics.record_day(col, &student)
score = f(student.true_mastery over the exam blueprint at test_date)
cost  = (total reviews, total simulated minutes)
```
Everything in CAPS is production code exercised unchanged: `build_queues`
(`queue/builder/mod.rs:308`; the reviewer-facing entry is `get_queued_cards`,
`queue/mod.rs:88`), `Collection::answer_card` (`answering/mod.rs:311`, driving
`answer_card_inner` `:315` via `CardAnswer` `:48` / `CardStateUpdater` `:67`), and the
governor/metamorphosis methods hung off that path.

### 6.4 Phasing (each phase is independently useful; stop anytime)

| Phase | Deliverable | Reuses | New code | Rough effort |
|---|---|---|---|---|
| **P0** | FSRS-knob answers (optimal DR, governor ramp shape, load-balancer) via `simulate_workload` sweeps | 100% existing | a few scripts | ~½ day |
| **P1** | Harness skeleton: seed test collection, driver loop, `StudentModel` trait + power-law truth, **baseline + oracle** arms, frontier + CI metrics | test-collection helpers, `answer_card` | crate scaffold, driver, metrics | ~3–5 days |
| **P2** | Concept DAG + confusability world; **gating** + **trickle-down** arms; starvation & over-credit instruments | `concept_edges`, gating filter, trickle-down path | `world/content.rs`, arms | ~3–4 days |
| **P3** | Interference term + ACT-R/disuse truths; **interleaving** arm; short-vs-long crossover check | `interleave.rs` queue path | 2 student models | ~4–6 days |
| **P4** | **Minting** + **metamorphosis** + **governor** arms; gap-causal mint model; exam-day scoring | minting seam, `metamorphosis.rs`, `governor.rs` | mint/gap model, scorer | ~3–4 days |
| **P5** | Calibrate population to public FSRS dataset (offline replay); full sensitivity sweep; kill-criteria report | `evaluate_params`, `ExportDataset` | calibration + sweep runner | ~3–5 days |

P0 is runnable now and answers the FSRS-knob questions legitimately. P1 is the real
investment; P2–P4 unlock the differentiators one at a time; P5 makes the numbers
defensible.

### 6.5 Verifying the harness itself (else we test a bug)
- **Null test:** two identical arms under common random numbers must produce
  statistically identical frontiers (any gap = harness leak).
- **Oracle bound:** no policy may beat the oracle; baseline must be beatable — if a
  feature "beats the oracle," the metric or the model is wrong.
- **Degenerate content:** with an empty prereq DAG and zero confusability, gating and
  interleaving must collapse to no-ops vs. baseline (recovers the stock regime).
- **Congruence check:** under the power-law truth *only*, FSRS knobs should look near
  optimal — if not, the harness mis-drives the scheduler.
- Unit-test the pure functions (`recall_prob`, forgetting curves, the confusability
  update) exhaustively, matching the `governor.rs` "pure + standalone, unit-tested"
  style.

### 6.6 Risks specific to the build
- **Seeding real collection state in Rust** (notes, tags, `concept_edges`) is more
  verbose than Python `provision.py`; mitigate with a small `world::seed(col, &content)`
  builder and keep the synthetic content declarative.
- **Clock control:** the scheduler reads "today" via `timing_today()`; the harness must
  advance the collection's notion of the current day deterministically (offset the
  creation/rollover), not wall-clock — and scripts already can't use `Date::now`.
- **fsrs-rs coupling:** the oracle and the FSRS-congruent truth both call into
  `fsrs-rs`; keep the `StudentModel` truths independent of the scheduler's `FSRS`
  instance so congruence stays a *choice*, not an accident.

---

## 7. What simulation cannot tell us (read before trusting a number)

- The **human magnitude** of spacing, interleaving, desirable-difficulty, and
  error-driven-minting effects — we *assumed* those in the student model.
- Motivation, fatigue, dropout, and whether the *minting UX* actually gets used.
- Whether our authored prereq graph and confusability matrix match reality — a wrong
  world produces confident wrong verdicts.

So the deliverable is explicitly **"effect under model M, across a sensitivity sweep,"**
and the pipeline ends by nominating the 2–3 survivors for a small real within-subject
pilot. Simulation is a **filter and a design tool**, not proof of human efficacy.

---

## 8. Open questions / decisions for the owner

1. **Harness language & placement** — new `sim/` Rust crate + `just sim` (recommended,
   drives shipped code fastest) vs. a Python harness over pylib (reuses
   `provision.py`, slower). *Recommend Rust.*
2. **Which truths to author first** — minimum for the differentiators is one
   disuse/ACT-R-style model *plus* the FSRS-congruent baseline; the other families are
   robustness insurance. How many before we trust a verdict?
3. **Content authoring** — the synthetic prereq DAG + confusability matrix is
   pedagogical work; reuse the MCAT concept graph shape, or hand-author a small,
   well-understood toy domain first for validation?
4. **Kill criteria** — copy the PRD M3 A/B thresholds verbatim into the sim, or set
   sim-specific (looser, since sim is a pre-filter) thresholds?
5. **Where results live** — CSV + a Svelte frontier view under `ts/routes/synapse/`,
   or a standalone research notebook outside the app build?
