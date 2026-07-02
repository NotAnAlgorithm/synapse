# Product Requirements Document — Synapse

**An MCAT study app built on the Anki core, engineered around learning science instead of card-count grinding.**

Version 0.1 (draft for internal review) · Status: proposal · Owner: e

> The name Synapse reflects what the product is built to do: strengthen the connections between concepts the way a synapse strengthens with repeated use. It also mirrors the app's knowledge-graph design, where concepts are nodes joined by prerequisite links, and it signals the memory-and-neuroscience heritage of the spaced-repetition engine underneath.

---

## 1. Vision and the one goal

We are building a study app whose single job is to raise a student's MCAT score as much as possible per hour studied. Not to maximize cards reviewed, not to maximize daily streaks, not to help someone "finish a deck." Those are the things existing tools optimize, and the evidence says they are mostly proxies that drift away from actual learning.

The product sits on top of the Anki Rust core (the `anki` crate in `rslib/`), which already gives us a battle-tested spaced-repetition engine (FSRS on the V3 scheduler), a sync system, a data model, and a shared codebase across desktop and mobile. We keep that engine and build a full MCAT studying ecosystem around it: error-driven card creation, application-level practice, a prerequisite knowledge graph, grounded AI, and honest readiness metrics.

The guiding sentence for every design decision: **does this move durable, transferable learning that shows up on a real AAMC full-length, or does it just move a number that feels like progress?**

---

## 2. The problem we are actually solving

The dominant MCAT Anki workflow is to download a 5,000+ card premade deck (AnKing, MileDown, JackSparrow) and grind it to completion. Our research surfaced four concrete reasons this underperforms, and each one maps to a feature later in this doc.

**Bare recall transfers weakly to the exam.** The MCAT almost never asks "what is X." It asks you to use X inside a novel experiment or passage, and CARS is pure reasoning with nothing to memorize. The best evidence here is Pan & Rickard's 2018 meta-analysis (192 effect sizes, ~10,000 participants): retrieval practice does transfer to application and inference, but only at about d = 0.40 overall, and that falls to d = 0.28 when the practiced answer doesn't overlap the tested one. It climbs to d = 0.58 with response congruency and gets another ~0.23 boost from "elaborated" retrieval. A 2023 preregistered replication (Mayrhofer et al.) also showed that a chunk of the famous testing-effect advantage over concept mapping was really an artifact of extra memorization time. Put together: plain cloze cards sit in the weak end of the transfer range, which is exactly the wrong end for a reasoning test.

**"Completion" is a vanity metric.** Across the cohort studies we checked (Wright State n=130, UNLV n=36, Wothe n=165), the Anki statistic that most reliably tracks exam performance is percent of cards *mature*, not card count, review count, retention rate, or ease. It isn't the *only* signal that correlates (the UNLV cohort found study hours and unsuspended-card count did too), but those lose independent significance once mature-card count is in the model, because they're collinear with it. The direction is what matters: hours ground and cards created are not the target. All of these studies are correlational and single-institution, and none used the MCAT, so we treat them as directional, not causal.

**Nothing schedules against the test date.** FSRS is deadline-agnostic by design; it optimizes for indefinite retention, not for peak recall on one specific Saturday. That is a real gap. Cepeda et al. (2008) showed the optimal spacing gap shrinks as the horizon to the test shrinks. The naive fix (lower your retention target early to "add difficulty") is actually a category error, and we explain why in Feature A1/A2.

**Ungrounded AI hallucinates badly.** The current wave of AI flashcard and quiz generators spins content out of a model's parametric memory. In an adversarial medical study (Omar et al. 2025), models elaborated a planted false detail in 50–82% of cases, and prompting only cut that to ~44%. Fabricated citations are rampant (Chelli et al. 2024: GPT-4 got 28.6% of references wrong, Bard 91.4%). Grounding the model in real source text drops error dramatically but never to zero. So AI has to be a grounded transformer with human review, not an oracle.

Our bet is that fixing these four things beats the *worst* existing workflow (mindless deck-grinding) comfortably, and that with the knowledge graph and grounded AI it can also beat the *disciplined* workflow that top scorers already use by hand (content review → practice → error-logged custom cards). That second claim we do not assume; it's the thing to prove, which is why several features ship as A/B experiments with kill criteria.

---

## 3. Design principles (the rules every feature answers to)

These come straight out of the research and act as guardrails.

1. **Train the criterion task.** The closer practice looks to a real MCAT passage question, the more it transfers. Prefer application items, "which principle applies" stems, and data snippets over bare definitions.
2. **Difficulty must be earned, not imposed.** A desirable difficulty only helps if the learner has the prerequisites to succeed at it. When retrieval fails, you get almost no learning. So difficulty gets titrated to per-concept mastery, never applied blindly.
3. **Spacing and interleaving are close to free wins.** They are among the most replicated effects in the field, they cost mostly acquisition speed (which students misread as the technique "not working"), and they map cleanly onto what FSRS already does.
4. **Ground the AI or don't ship it.** AI may transform vetted content. It may not originate facts. Every generated item cites its source and passes a quality gate before a student sees it.
5. **Measure learning, not activity.** The scores we surface are Memory (can you recall it), Performance (can you apply it to a novel item), and Readiness (your projected score), each shown with an honest range. Card counts, hours, and streaks are deliberately de-emphasized.
6. **Solve adoption without corrupting the objective.** An app nobody opens teaches nothing (Khanmigo, an excellent grounded tutor, sees only ~15% of eligible students engage). We use engagement mechanics, but we avoid the pure streak/loss-aversion loop that pushes people to speed-run easy reps.
7. **Be honest about weak evidence.** Where a design leans on correlational, vendor, adversarial, or simulation-derived data, we say so, and we build the experiment to check it rather than shipping the belief.

---

## 4. Target users and personas

The MCAT population is not monolithic. We designed four personas; **Maya is the primary persona** and the app must be excellent for her first.

### Persona 1 — Maya Chen, the traditional pre-med (PRIMARY)

- **Age / stage:** 21, junior at a state university, studying during a dedicated 12-week summer window.
- **Target:** 515 (she's scoring ~508 on early practice).
- **Context:** Studies 6–8 hours a day. Uses the AnKing deck plus UWorld. Conscientious and motivated.
- **Pain points:** Her Anki review pile has ballooned to 600+ cards a day and she spends more time on cards than on passages. She *knows* the content when she sees a flashcard, then blanks when the same fact is buried in a passage with a graph. She feels busy but her practice scores have plateaued.
- **Behaviors:** Checks her streak. Feels guilt about "unfinished" decks. Reviews in long blocks by subject.
- **What success looks like for her:** Fewer, smarter reviews; cards that actually show up as points on AAMC FLs; a clear signal of whether she's on track for 515 by her test date.
- **Features that serve her:** deadline-aware scheduling (A2), interleaving (A3), application items (B2), error-driven minting (B1), a placement check when she switches in (D3), and an honest scores dashboard showing Memory, Performance, and Readiness (E1, F1–F3).

### Persona 2 — Devin Okafor, the non-traditional retaker

- **Age / stage:** 28, career changer, working 25 hours a week, retaking after a 505.
- **Target:** 510+ to be competitive, with a hard test date in 10 weeks.
- **Context:** Time-poor. Strong in psych/soc from a psychology undergrad, genuinely weak in physics and gen chem.
- **Pain points:** Can't afford to review things he already knows. Needs the app to be ruthless about spending his limited hours on his real gaps. Burned out last cycle from grinding.
- **Behaviors:** Studies in short scattered sessions. Skips content he's confident in (sometimes wrongly).
- **What success looks like for him:** Maximum score gain per hour; the app protecting him from wasting time; targeted remediation of physics without re-teaching psych.
- **Features that serve him:** knowledge graph + mastery gating (D1/D2), trickle-down review credit (D2), a placement assessment so he skips re-studying psych/soc (D3), coverage checker against the AAMC outline (B4), leech repair on his physics confusions (C3), efficiency-first scheduling (A1).

### Persona 3 — Priya Nair, the 520+ striver

- **Age / stage:** 22, senior, gunning for a top-tier MD/PhD.
- **Target:** 520+ (scoring ~516).
- **Context:** Very strong content knowledge. Her remaining gains are in CARS and in the last mile of application under time pressure.
- **Pain points:** Most tools waste her time re-drilling facts she mastered months ago. She needs harder, transfer-level practice and help closing small, specific gaps. Generic decks are beneath her.
- **Behaviors:** Self-directed, skeptical of gimmicks, will abandon anything that feels like busywork.
- **What success looks like for her:** The app recognizes mastery and stops making her review it, promotes her to application/passage practice, and surfaces the two or three concepts actually holding her back from 520.
- **Features that serve her:** card metamorphosis done right (B3), expertise-aware scheduling so mastered recall cards fade to long intervals (A1/B3), a placement assessment that credits what she already owns (D3), application + CARS practice (B2), precise gap surfacing via the tutor and the Performance score (C2, F2).
- **Why she matters:** She's the expertise-reversal case. She's the reason we do NOT blindly keep drilling mastered recall, and also the reason we do NOT hard-delete it either (see B3).

### Persona 4 — Jordan Reyes, the early/exploratory starter (SECONDARY)

- **Age / stage:** 20, sophomore, 9 months out, just starting content review.
- **Target:** undecided, building foundations.
- **Context:** Novice on most MCAT content. Working memory gets overloaded fast on new material.
- **Pain points:** Doesn't yet know what he doesn't know. A wall of 5,000 cards is demoralizing and, for a novice, the wrong tool. Needs scaffolding and worked examples before open practice.
- **What success looks like for him:** A guided on-ramp that builds prerequisites in order, with worked examples first, before it starts throwing hard application items at him.
- **Features that serve him:** diagnostic seeding + coverage deck (B4), worked-example-first sequencing for novices (D1, drawing on cognitive-load research), gentle mastery gating (D2).

---

## 5. User stories

Grouped by epic. Format is the usual "as a [persona], I want [X] so that [Y]," with a note on what "done" roughly means.

### Epic A — Scheduling that respects my test date and my time

| # | Story | Done means |
|---|---|---|
| A-1 | As Maya, I want reviews scheduled so I'm not doing needless reps on cards I clearly know, so I can spend more time on passages. | FSRS runs at its workload-efficient retention; no artificial early-difficulty inflation. |
| A-2 | As Devin, I want the app to compress my schedule as my test date nears so my recall peaks on exam day, not next month. | A "test-date governor" ramps effective retrievability up in the final weeks; validated by A/B before default. |
| A-3 | As Maya, I want my reviews mixed across subjects and question types rather than blocked, so I learn to pick the right approach the way the real exam demands. | Queue builder interleaves by concept/type; block-by-subject is not the default. |
| A-4 | As Devin, I want to enter my test date and have everything schedule backward from it. | Test date is a first-class setting that drives the governor and the readiness projection. |

### Epic B — Cards that match what the MCAT tests

| # | Story | Done means |
|---|---|---|
| B-1 | As Maya, when I miss a practice question, I want a card minted from that exact gap and linked to the question, so my reviews target real weaknesses. | Missing an item offers a one-tap "make a card from this," with lineage stored. |
| B-2 | As Priya, I want application and passage-style items, not just definitions, so my practice looks like the test. | Item types include passage-embedded stems, "which principle applies," and explain-why prompts. |
| B-3 | As Priya, once I've truly mastered a fact, I want the app to stop drilling the bare recall version and promote me to applying it, without silently letting the fact rot before my exam. | Recall card fades to long intervals and an application item is added; recall is retired only after application mastery, with light maintenance retrieval kept before the exam. |
| B-4 | As Jordan, I want an optional vetted deck for coverage and a checker that flags high-yield topics I've never made a card for, so I don't have blind spots. | Opt-in premade deck available; coverage report maps my cards against the AAMC content outline. |

### Epic C — AI I can actually trust

| # | Story | Done means |
|---|---|---|
| C-1 | As Maya, I want AI-generated cards and explanations to be grounded in real, cited source material, so I'm not memorizing hallucinations. | Every AI item cites its grounding source and passes an automated flaw check plus human review before publish. |
| C-2 | As Devin, I want a tutor that knows my mastery map and walks me through *why* I got something wrong, surfacing the prerequisite I'm missing, without just handing me the answer. | Socratic tutor grounded in student state; guardrailed against answer-giveaway; surfaces unmastered prerequisites. |
| C-3 | As Devin, when a physics card keeps failing, I want the app to notice early, figure out what it's confusing it with, and re-teach it, instead of just showing it to me harder. | At the first couple of lapses the card is atomized, the interfering sibling is detected, and a short worked-example micro-lesson runs before rescheduling. |

### Epic D — A structure that knows what depends on what

| # | Story | Done means |
|---|---|---|
| D-1 | As Devin, I want the app to understand that amino-acid chemistry underlies enzyme kinetics, so it teaches me in the right order and doesn't drop me into hard material I'm not ready for. | Content is mapped to a prerequisite knowledge graph aligned to the AAMC outline; per-concept mastery is tracked. |
| D-2 | As Devin, when I nail an advanced problem, I want partial review credit to trickle down to its prerequisites, so I'm not redundantly reviewing the basics. | Advanced practice grants discounted implicit credit up the prerequisite chain (FIRe-style). |
| D-3 | As Priya, I want the two or three specific concepts blocking my next score jump surfaced, not a generic to-do list. | Mastery map + practice data identify the highest-leverage weak concepts. |
| D-4 | As Devin, when I join after months of studying elsewhere, I want a short placement check that credits what I already know so I don't redo it. | An optional adaptive placement assessment seeds per-concept mastery and skips confirmed material. |
| D-5 | As Priya, I don't want placement to mark something "known" from one lucky answer and then hide it from me forever. | Mastery credit requires application-level and spaced confirmation, not a single correct response; borderline items get a shortened interval, not retirement. |

### Epic E — Motivation that rewards the right thing

| # | Story | Done means |
|---|---|---|
| E-1 | As Maya, I want my home screen to show mature, test-ready concepts and my projected score, not a card count or a flame, so I'm chasing learning, not activity. | Dashboard leads with the three scores (Memory, Performance, Readiness) and de-emphasizes card counts and streaks. |
| E-2 | As Maya, I don't want to lose everything because I missed one day during exam crunch. | Streak-freeze/forgiveness exists; adherence mechanics don't punish a single miss. |
| E-3 | As Devin, I want to be rewarded for tackling my weak areas and recovering lapsed cards, not for speed-running easy ones. | Points/recognition weight difficult successful retrievals and recoveries; easy-rep padding earns little. |

### Epic F — Knowing where I stand (Memory, Performance, Readiness)

| # | Story | Done means |
|---|---|---|
| F-1 | As Maya, I want to see how likely I am to recall each concept right now. | A Memory score (FSRS retrievability) shown per concept, per section, and overall. |
| F-2 | As Priya, I want to know whether I can actually apply a concept to a new exam-style question, not just recall it. | A Performance score estimating success on unseen application items, shown with a wider range than Memory and clearly distinguished from it. |
| F-3 | As Maya, I want a projected MCAT score with an honest range so I can decide whether to sit or reschedule. | A Readiness projection anchored on AAMC full-lengths, shown with a range, confidence reduced and flagged above ~515. |
| F-4 | As Devin, when I enter a third-party practice score, I want it adjusted toward a realistic AAMC-equivalent. | Documented third-party offsets applied with caution and clearly labeled as estimates. |
| F-5 | As Devin, I'd rather see "not enough data yet" than a confident score the app can't back up. | Any score with too little coverage or too wide a range is withheld and shows a clear "keep practicing" state instead. |

---

## 6. Product features (detailed, with the science and the implementation)

Each feature below gives: what it is, the learning-science basis with evidence and honest caveats, how it works on the Anki architecture, and how we'll know it worked. Features are grouped A–F to match the epics.

### Group A — The scheduling engine

#### A1. FSRS at workload-efficient retention (and NOT lowering retention to fake difficulty)

**What it is.** We keep FSRS as the scheduler and run it at roughly its workload-minimizing desired retention (about 0.85–0.90). We do not expose "lower your retention early to make retrieval harder" as a feature, because it's a mistake.

**The science.** Two things get conflated in the wild. Bjork's desirable-difficulty idea is that a *successful* retrieval from a lower retrieval-strength state builds more durable memory. That is real. But the lever for it is the *spacing gap*, which FSRS already operationalizes: it schedules a card for review right when your predicted recall has decayed to the target. Lowering the FSRS *desired-retention parameter* is a different lever. The FSRS developers' own workload analysis shows the relationship between desired retention and total workload is U-shaped: push retention very high and you review constantly; push it too low and you review constantly too, because cards lapse and you pay the relearning tax. Their guidance is explicit that setting the target below the recommended value is "actively detrimental for studying efficiency." There's also a gap between the parameter and reality: at a 0.90 desired-retention setting, true average retrievability across your cards runs higher (around 0.95), because reviews fire at the due moment, not continuously. So "lower retention = more desirable difficulty" doesn't even do what it claims.

**Caveat.** The workload-optimal figure (~0.85) comes from large-scale simulation on aggregate community data, not a controlled MCAT trial. The true optimum varies per learner and per deck, so we let it adapt rather than hard-coding one number.

**How it works on the architecture.** FSRS already lives in the Rust core at `rslib/src/scheduler/fsrs/`, layered on the V3 scheduler, with per-card memory state (`memory_state`: stability, difficulty, plus `desired_retention`, `decay`, `last_review_time`) serialized in the `cards.data` JSON column, and `desired_retention` also configurable at the `DeckConfig` level (`rslib/src/deckconfig/mod.rs`). We inherit all of this. Our change is mostly a matter of setting sane defaults and *not* building the anti-feature.

**Success.** Equal-or-better practice-test scores at a lower total review load than a flat-0.90 baseline, measured on our own users.

#### A2. Test-date governor (deadline-aware scheduling, shipped as an experiment)

**What it is.** A student enters their exam date. As the date approaches, the scheduler shifts so that recall is highest *on test day*. Concretely, it raises effective retrievability and compresses intervals in the final ~2–3 weeks rather than holding a flat indefinite-retention target.

**The science, stated carefully.** This is a real gap: FSRS optimizes for indefinite retention, not a fixed date. Cepeda et al. (2008) is the anchor, and it must be cited honestly. It tested the optimal gap between *two* study sessions given a single fixed test delay, and found that gap shrinks as the horizon shrinks (roughly 20–40% of a one-week delay, down to 5–10% of a one-year delay). It did *not* test a multi-review SRS curve, and it is not a validated schedule for "compress everything as the deadline nears." So we use it as motivation for the direction (peak the curve at the deadline), not as proof of a specific curve. Critically, the defensible move is to raise retrievability *late*; the opposite move, lowering the retention target early to manufacture difficulty, just buys lapses (see A1).

**Caveat and the reason it's an experiment.** There is no RCT of a deadline-aware retention curve. We ship it behind an A/B test against flat FSRS. **Kill criterion:** if the deadline arm shows higher *total* review load (including relearning) OR lower practice-test scores, we cut it.

**How it works on the architecture.** This is a scheduler change in the Rust core (`scheduler/answering/` and `scheduler/queue/`), reading a new `test_date` field. Because the Rust core is shared with mobile via the JNI bridge (`rslib-bridge`), one implementation covers desktop and AnkiDroid at once. We add `test_date` to the collection config (new field in `proto/anki/config.proto` or the deck config), which flows through codegen to all layers.

**Success.** Higher projected and actual readiness on exam day at equal or lower total workload, in the A/B.

#### A3. Interleaving by default

**What it is.** Review sessions mix concepts, disciplines, and question types instead of blocking one subject at a time. Students can't just grind all of biochem then all of physics.

**The science.** This is one of our strongest planks because it rests on a large classroom RCT, not just lab work. Rohrer et al. (2020) randomized 787 seventh-graders across 54 classes; on an unannounced test a month later, interleaved practice scored 61% versus 38% for blocked, d = 0.83. The mechanism is that interleaving forces you to *choose* the right strategy based on the problem itself, which is exactly the discrimination the MCAT rewards when it mixes disciplines in one passage. Interleaving also bakes in spacing (Cepeda et al. 2006 meta-analysis: spaced ~47% vs massed ~37%). The known cost is slower acquisition and lower practice-time accuracy, which students misread as the method failing (Rohrer & Taylor's "practice-vs-learning reversal"). We'll message this so users don't bail.

**How it works on the architecture.** Queue construction is in `rslib/src/scheduler/queue/`. Today Anki can mix cards across a deck, but we make concept/type interleaving the intentional default rather than letting users block by subject deck. The knowledge-graph concept tags (Group D) feed the interleaving logic so the mix is pedagogically sensible (interleave *related* discriminable concepts, not random noise).

**Success.** Better transfer/application scores for interleaved cohorts, accepting slightly slower early accuracy.

### Group B — The card lifecycle

#### B1. Error-driven card minting

**What it is.** The primary way cards come into existence is from missed practice questions. Miss an item, get a one-tap "mint a card from this," and the new card is permanently linked to the question that spawned it.

**The science.** This aligns with instructor consensus (Jack Westin: "Practice finds weaknesses. Anki prevents re-forgetting"; tie cards to passage mistakes) and with the transfer literature: retrieval transfers best when the practiced item resembles the tested one (Pan & Rickard's response congruency, d up to 0.58). It also matches Deng et al. (2015), where practice questions bought Step 1 points about as efficiently as flashcards (roughly one point per 445 questions vs one per 1,700 cards), hinting that question-anchored study is potent. Honest note: "questions first, cards downstream" is good practice but it is *not* novel; top scorers already do this by hand. Our contribution is making it frictionless and automatically linked, not inventing the idea.

**Important design choice.** Minting is *additive*, not exclusive. A pure error-only system has a coverage-gap risk: you never make cards for high-yield topics you happen not to have been tested on yet, and psych/soc in particular is broad enough that this bites. So we pair minting with the coverage tools in B4 and the placement seeding in D3.

**How it works on the architecture.** A missed-question event creates a note through the normal path (`Collection::add_note` → `add_note_inner` in `rslib/src/notes/mod.rs`), which already generates cards from the notetype templates. The link back to the source question is stored in the card's `custom_data` JSON (a field the core already exposes on every card for exactly this kind of add-on state) and/or a new `card_lineage` table if we want it queryable. AI drafts the card from the (grounded) explanation text (see C1).

**Success.** Minted cards show higher downstream accuracy on re-tested passages than generic deck cards.

#### B2. Application-form items and elaborated retrieval

**What it is.** New item types beyond bare cloze: passage-embedded stems, "which principle applies here," data-snippet questions, and explain-why prompts that ask for reasoning.

**The science.** This is the core lever for a reasoning exam and it's multiply supported, but one terminology point has to come first because it's easy to trip on: "elaborated retrieval" is not the same thing as the "elaborative study" that *loses* to testing. Karpicke & Blunt (2011) pitted retrieval practice against elaborative concept mapping, which is elaboration used as a study/encoding strategy (building maps with the material in front of you, instead of retrieving), and retrieval won even on inference questions. That result tells us the retrieval act itself is the engine, not the elaborating (originally d ≈ 1.50, though the 2023 Mayrhofer replication shows that number was inflated by a memorization-time confound, so we cite the *direction*, not the 1.50). "Elaborated retrieval" is a different move: it means enriching the retrieval act itself with explanation prompts, higher-Bloom questions, and elaborative feedback. Pan & Rickard found this *adds* to transfer on top of plain retrieval (in their meta-analysis transfer climbs from d = 0.28 to 0.58 once the practiced and tested responses overlap, with roughly another 0.23 on top for elaboration). So the two findings agree rather than clash: elaboration used *instead of* retrieval is weak, but elaboration layered *onto* retrieval is strong. Willingham adds that reasoning is bound up with domain knowledge and that transfer comes from varied practice across surface forms, not a generic "critical thinking" module. The net instruction is to push retrieval up Bloom's levels rather than capping it at recognition.

**How it works on the architecture.** New notetypes and card templates (`rslib/src/notetype/`, `template.rs`, `card_rendering/`) support richer stems, embedded passages, and short-answer/explanation capture. Rendering already runs through the Rust template engine shared across platforms. Grading an explanation prompt can use the grounded AI layer (C1) to check against a rubric, with the student's own recall of the reasoning being the point.

**Success.** Higher accuracy on held-out application/passage items for students trained on application cards vs recall-only.

#### B3. Card metamorphosis, done the way the evidence actually supports

**What it is.** As a fact is mastered, its bare-recall card fades to long intervals and an application item covering the same concept is introduced. Mastery is judged on the *application* form. But we do NOT hard-delete the recall substrate, and we keep light maintenance retrieval before the exam.

**Why it's built this way.** It would be tempting to *expire* a recall card once the fact is mastered, on the logic of the expertise-reversal effect. That would be a misapplication. Expertise reversal (Kalyuga, Sweller) is about *instructional guidance* (worked examples, redundant explanations) becoming unnecessary or harmful for experts. Retrieval practice is not instructional guidance; it's the desirable difficulty itself, and nothing in expertise reversal says stop retrieving mastered facts. Worse, Bjork's New Theory of Disuse says retrieval strength decays without use, so a "mastered" fact whose card you retired will quietly rot, which is dangerous before a fixed-date high-stakes exam. And "the MCAT never asks what X is" is overstated: psych/soc rewards precise discrimination between near-identical terms, and amino acids, hormones, and equations are legitimately recall. So we metamorphose by *adding* application on top and letting FSRS stretch the recall interval naturally (which already makes mastered cards cheap to maintain), rather than by deletion.

**How it works on the architecture.** FSRS already grows intervals for stable cards, so a mastered recall card costs almost nothing to keep. "Mastery on the application form" is a threshold on the application item's FSRS stability plus accuracy, tracked via the knowledge graph (Group D). Card lineage (recall card → application card for the same concept) lives in `custom_data` or the lineage table. Retirement, if it happens at all, is a suspend (`CardQueue::Suspended`), which is reversible, not a delete, and we schedule occasional maintenance reviews in the pre-exam window via the governor (A2).

**Success.** Mastered concepts stay retrievable through test day (no measurable pre-exam decay) while total reviews on those concepts drop.

#### B4. Opt-in premade coverage deck and AAMC coverage checker

**What it is.** A vetted premade deck is available as an *optional* coverage backbone, and a coverage report maps a student's existing cards against the official AAMC content outline to flag high-yield topics with no card.

**The science / product logic.** "Completion" is a vanity metric, but that doesn't mean premade decks are useless. They're a proven, zero-setup, high-coverage tool, and there are real existence proofs of students scoring in the 520s using essentially a premade deck plus practice. Removing that backbone entirely creates a cold-start problem and a coverage-gap risk, especially for novices (Persona Jordan) and for broad sections like psych/soc. Keeping the deck opt-in, while making error-driven minting the *default* path, preserves coverage without making "finish the deck" the goal.

**How it works on the architecture.** A premade deck is just a shared deck (standard Anki import/export, `import_export` proto + `rslib` importers). The coverage checker is a mapping layer: each card is tagged to one or more AAMC content categories (via the knowledge graph, Group D), and we compute which categories are underrepresented. This is a new read-model/query, surfaced in the web UI (a SvelteKit page under `ts/routes/`), reachable over the allow-listed HTTP path.

**Success.** Fewer "topic I never studied" surprises on full-lengths for users who act on the coverage report.

### Group C — The grounded AI layer

#### C1. Grounded generation only (RAG + rule-based flaw check + human-in-the-loop)

**What it is.** AI is sandboxed to source-grounded transformation: summarize *this* AAMC-aligned passage into atomic cards, write distractors from *this* verified answer key, explain *this* correct solution. It never originates facts. Every generated item carries a citation to its grounding source and must pass an automated item-flaw check plus human expert review before it reaches a learner.

**The science, with the numbers put in their correct context.** The hallucination literature looks contradictory until you sort it by task. The scary ~50–82% figure is Omar et al. (2025), an *adversarial* study where each prompt had a fabricated detail deliberately planted; that's a worst-case, not a base rate for generation, and prompting only reduced it to ~44%. The reassuring ~1.47% figure is Asgari et al. (2025), but it comes with two easily-missed caveats: it's a *summarization* task, and it used direct GPT-4 prompting, *not* RAG, so it isn't evidence that "RAG is safe," it's evidence that constrained summarization is easier. Reference fabrication is real and large (Chelli et al. 2024). And AI-written MCQs, while comparable to human ones on average (Doughty et al. 2024; psychometric parity in npj Digital Medicine 2025), carry higher rates of specific defects: multiple-correct answers and answer-giveaway distractors, roughly 4–5% vs ~1% for humans. Notably, an LLM is a *worse* judge of item quality than a rule-based checker (catching ~79% vs ~91% of item-writing flaws), which is why our quality gate is rule-based first, not "ask another model."

**Honest cost.** Grounding is a real operational burden: it needs a vetted source pipeline, it's slower, and it covers less than free-form generation. We accept that cost because one confidently-wrong card on a high-stakes exam is worse than a smaller, slower, trustworthy library. Grounding reduces hallucination sharply but never to zero (best clinical RAG configs still sit around 5–6%), so we set an explicit residual-hallucination monitoring target rather than pretending it's solved.

**How it works on the architecture.** The AI pipeline is a *service layer* concern, not something we bury in the Rust core (the core has no business making LLM network calls). Generation happens in a backend service that retrieves from the vetted source corpus, drafts items, runs the rule-based flaw checker, routes to human review, and only then writes approved notes into the collection through the normal `add_note` RPC. On mobile the same content syncs down through the existing sync path. Each item's grounding citation is stored on the note (a dedicated field) so it's always visible.

**Success.** Post-review defect and hallucination rates at or below the human item-writer baseline, with source citations on 100% of AI items.

#### C2. AI tutor grounded in student state

**What it is.** A Socratic tutor that walks a student through *why* they missed something, wired into their per-concept mastery map and the knowledge graph, and specifically surfacing the unmastered prerequisite behind a mistake. It's guardrailed against just giving the answer.

**The science.** Khan Academy's Khanmigo is the best evidence, and it's clean A/B data even if it's company-reported. Grounding the tutor in the student's learning record produced +6.1% next-item correctness overall, with +3.4% from feeding it problem-solving history (608k threads) and +2.7% from surfacing unmastered prerequisites (1.36M threads). "Next-item correctness" is a genuine transfer metric (did the student then solve the *next* problem unaided), not AI-assisted performance. The lesson isn't "chatbots are magic"; it's that the tutor's value scales with how well it's grounded in (a) the correct answer and (b) the learner's mastery map, the same grounding principle as C1 applied to dialogue.

**The adoption caveat, which drives the design.** Khanmigo, an excellent grounded tutor, sees only ~15% of eligible students engage with it, which prompted a proactive-redesign. So we make the tutor proactive and low-friction (it shows up at the moment of a mistake, in context) rather than a free-floating chat the student has to remember to open.

**How it works on the architecture.** Also a service-layer feature. The tutor reads the student's mastery map (Group D) and recent revlog, and is grounded in the verified explanation for the specific item. It's surfaced in the reviewer UI at the point of error. Guardrails (don't reveal the answer) are prompt- and policy-level in the service.

**Success.** Measurable lift in unaided next-item correctness for students who use the tutor after a miss, plus engagement well above the ~15% Khanmigo floor.

#### C3. Leech repair (early intervention, teach instead of suspend)

**What it is.** Instead of waiting for 8 lapses and then just tagging/suspending a "leech," the app intervenes at the first couple of lapses: it atomizes the card, detects the sibling it's interfering with, runs a ~60-second worked-example micro-lesson, then reschedules.

**The science.** Anki's own manual says a leech usually signals a poorly written or not-yet-understood card, often from interference between similar items (the classic "disappoint" vs "disappear"). Wozniak calls interference "probably the single greatest cause of forgetting" in a mature collection and prescribes atomizing and disambiguating, not brute-force rescheduling. For a novice on a confusable pair (amino acids, hormones, hydroxyl vs carbonyl), a worked example lowers cognitive load and builds the schema, which is exactly what cognitive-load research (Sweller; Kirschner, Sweller & Clark) says novices need before more retrieval. So a chronically failing card is diagnostic: the right response is to reformulate or re-teach, not to shrink the interval.

**How it works on the architecture.** Lapse handling and leech tagging already happen inside `answer_card_inner` in the Rust core (leech tag added when the lapse threshold trips; threshold configurable in `DeckConfig`). We lower the intervention threshold and, on trigger, fire a hook that hands off to the AI service to (a) detect the interfering sibling by searching the collection for confusable notes and (b) generate a grounded micro-lesson. The atomized replacement cards go back in through `add_note`.

**Success.** Lower eventual lapse counts and higher stability on repaired cards versus cards left to the default 8-lapse suspend.

### Group D — Knowledge graph and mastery

#### D1. Prerequisite knowledge graph aligned to the AAMC outline

**What it is.** MCAT content is modeled as a connected prerequisite graph (concept nodes with "depends-on" edges), mapped onto the official AAMC content categories, with continuous per-concept mastery tracked for each student.

**The science.** Several strands converge here. Willingham: reasoning is intertwined with domain knowledge, and transfer is rare without it, so structure matters. Bjork: a difficulty is only *desirable* if the learner has the prerequisites, so you need to know what those prerequisites are. Pan & Rickard: transfer collapses toward zero when initial retrieval is poor, i.e., when a prerequisite isn't in place. And Math Academy's model (Skycak) treats a STEM curriculum as a prerequisite graph with mastery gating and regenerated, non-memorizable questions. Bloom's mastery-learning work (the "2 sigma" result) supports the direction that mastery-based sequencing helps, though modern replications land well below 2 sigma, so we take the direction, not the headline number. This makes the knowledge graph a correctness requirement for applying the rest of the science safely, not a nice-to-have.

**Caveat.** Math Academy's specific efficiency claims (e.g., "4x speed") are company marketing and aren't independently validated. We borrow the *architecture*, not the number.

**How it works on the architecture.** This is genuinely new data, so it earns new tables. Anki's tag system is a flat namespace and won't represent edges. We add `concepts`, `concept_edges` (prerequisite links), and `card_concepts` (card↔concept mapping) tables via a schema migration (`rslib/src/storage/`, `schema11.sql` base is at v18; migrations live in `storage/upgrades/`), plus matching proto messages so all layers see them. Per-student mastery is derived from FSRS stability and accuracy per concept, cached in a read-model.

**Success.** The graph correctly predicts which prerequisites, when weak, drag down performance on downstream concepts.

#### D2. Mastery gating and trickle-down (implicit) review credit

**What it is.** Application/interleaved items for a concept are gated behind adequate mastery of its prerequisites (so we don't hand Devin a hard item he can't succeed at). And when a student succeeds on an advanced item, discounted review credit trickles down to its prerequisite concepts, so they don't get redundantly reviewed.

**The science.** This is Math Academy's Fractional Implicit Repetition (FIRe) idea: advanced practice gives partial "trickle-down" credit to prerequisites, and reviews are chosen so each one's implicit repetitions knock out other due reviews. It operationalizes two things we already believe: Khanmigo's +2.7% from surfacing unmastered prerequisites, and Bjork's rule that difficulty must be earned. Gating prevents the "undesirable difficulty" failure mode (a hard item with d ≈ 0 transfer because the student wasn't ready). Trickle-down attacks the review-pileup problem that burns out students like Devin.

**Caveat.** FIRe is a practitioner model, not independently replicated, and trickle-down credit is a heuristic. We'll tune the discount empirically rather than trusting a fixed formula.

**How it works on the architecture.** This is a scheduler-adjacent change in the Rust core. When a card is answered, in addition to the normal FSRS update in `answer_card_inner`, we apply fractional credit to prerequisite concepts (adjusting their effective due dates / stability) using the `concept_edges` graph. Because it's in the shared core, desktop and mobile get it together. Gating is a queue-building filter (`scheduler/queue/`) that withholds application items whose prerequisites are below threshold.

**Success.** Lower total review load at equal mastery, and higher success rates on newly unlocked application items (evidence the gating picked the right moment).

#### D3. Placement / diagnostic assessment (optional, for students who arrive with prior study)

**What it is.** An optional adaptive assessment a student can take when they join, aimed at anyone who has already done content review or practice elsewhere. It estimates per-concept mastery, then seeds the knowledge graph and FSRS so the student skips confirmed material and starts where their real gaps are. It's short and adaptive, not a 5,000-card slog.

**The science.** The "assess what you know, teach only the gaps" model has the strongest real-world track record in knowledge-space systems like ALEKS: a 2024 evaluation of 116,276 placement assessments reported classification AUROC ≈ 0.89, with topics marked "known" answered correctly ~83% of the time versus ~8% for "unknown," and placement scores correlating 0.75 with an independent initial score. Adaptive testing (IRT-based computerized adaptive testing) is efficient too: it typically reaches equivalent-or-better measurement precision in about half the items of a fixed-length test, using a standard-error stopping rule. Evidence caveat worth stating plainly: much of the ALEKS efficacy data is vendor-authored, and rigorous proof that placement reduces redundant study specifically in high-stakes exam prep is thin, so we treat our own rollout as a chance to generate that evidence (A/B placement-credited vs full-review cohorts).

**The critical design rule: never credit mastery on a single correct answer.** This is where the app's whole thesis bites. Performance in the moment overstates durable learning (Soderstrom & Bjork), and the illusion of competence means a fluent-feeling correct answer is an unreliable signal (Koriat & Bjork). A placement test that marks a concept "known" from one lucky recall would reintroduce exactly the false confidence the app exists to fight. So mastery credit requires (a) an application-level item, not bare recall; (b) at least one spaced/delayed confirmation before a concept is fully skipped; and (c) tolerance for slips (even "known" ALEKS topics miss ~17% of the time from careless error). Anything short of that seeds FSRS with a shortened-but-nonzero interval (partial credit), not full mastery.

**How it works on the architecture.** The item-selection and stopping logic (an IRT ability estimate with a standard-error / predicted-standard-error-reduction stopping rule) sits in a placement service. Because it writes results as per-concept mastery, it targets the same `concepts` / `card_concepts` tables and read-model from D1. Crediting a concept sets the initial FSRS memory state (`memory_state` on the relevant cards) to a partial or confirmed value instead of the default new-card state, so the scheduler just picks up from there: confirmed items get long intervals or a suspend, borderline items get a short seed interval. The flow reuses the existing card and scheduler machinery rather than building a parallel one.

**Success.** Students who take placement reach the same retention on credited concepts as students who studied them in-app (no elevated failure rate on delayed checks), while spending materially less time re-covering known material. **Kill/adjust criterion:** if more than ~10% of credited concepts fail their delayed confirmation, raise the confirmation bar.

### Group E — Motivation and engagement

#### E1. Learning-aligned dashboard

**What it is.** The home screen leads with the three scores from Group F (Memory, Performance, Readiness) plus a short "work on this next" list of the highest-leverage weak concepts. Card counts, hours studied, and streaks are demoted or hidden.

**The science.** Percent mature is the most robust Anki correlate of exam performance in the cohort data (correlational, single-site, non-MCAT, so directional), and the performance-vs-learning literature (Soderstrom & Bjork; Koriat & Bjork) shows fluency, confidence, and activity counts are unreliable indices of durable learning that can move opposite to it. Khan Academy's decision to optimize unaided next-item correctness is our template for a metric that reflects transfer, which is exactly what the Performance score (F2) captures. Learning-analytics reviews are sobering here: dashboards frequently fail to improve outcomes and most never explain their numbers, so we show the drivers behind each score rather than a bare figure. Net: display what tracks learning, explain it, and hide what students optimize by reflex.

**How it works on the architecture.** A read-model plus a dashboard page (`ts/routes/`, shared web UI). The three scores come from the Group F analytics; the "work on next" list comes from the mastery map (Group D). No new scheduler logic, just presentation over existing read-models.

**Success.** Users report chasing the three scores over card counts, and behavior shifts accordingly (less easy-rep padding).

#### E2 / E3. Adoption mechanics that don't backfire (shipped as an experiment)

**What it is.** We keep enough engagement scaffolding to fight the adoption ceiling, but we design against the failure mode. That means streak-freeze/forgiveness (a single missed day during crunch doesn't wipe progress) and rewards weighted toward *difficult* successful retrievals and recovering lapsed cards, rather than points for speed-running easy reps.

**The science, held at arm's length.** Streaks and loss-aversion mechanics are among the most robust *adherence* drivers known (Duolingo reports a streak feature lifting Day-7 retention ~14%, and users past a 7-day streak being several times more likely to keep going, though these are vendor metrics about engagement, not learning). But the same analyses note speed-running an easy lesson to protect a streak "activates the retention mechanism without the learning," and Shortt et al.'s systematic review found Duolingo's gamification mainly helped lower-order vocabulary, not advanced skill. There's also an overjustification risk: make studying contingent on a streak and a broken streak triggers abandonment, which is poison for a months-long prep cycle. So the tension is real: we need adherence (an app nobody opens teaches nothing, the single biggest product risk in the whole proposal) but pure streak-maxxing corrupts the objective.

**Why it's an experiment.** "Reward desirable difficulty instead of streaks" is a hypothesis with no outcome evidence behind it. We A/B it against a conventional low-pressure streak. **Guardrail/kill criterion:** if the anti-streak arm materially reduces daily/weekly active use or 4-week retention, we revert to the gentler streak. Adherence metrics gate this feature, not learning purity alone.

**How it works on the architecture.** Mostly app-layer (points, dashboard, notifications) reading from the revlog and mastery data. "Difficult successful retrieval" is derivable from FSRS state at answer time (a success on a low-retrievability card). Streak-freeze is app-state.

**Success.** Adherence at or above a conventional streak baseline *and* less easy-rep padding, in the A/B.

### Group F — Analytics: the three scores

The app computes three distinct scores, each at three levels of granularity: per concept, per section, and whole-exam. They're deliberately separate numbers because they answer different questions and routinely diverge, and showing that divergence is itself the pedagogical point. A student can have high Memory and low Performance, and hiding that behind one "mastery" number would recreate the illusion of competence the app is built to fight. Scoring at every level also guides the algorithms: section- and exam-level rollups tell the scheduler and the "work on next" list where the leverage is.

Every score, at every level, ships with the same display contract (F4): a point estimate, a likely range, the percent of relevant content covered so far, a plain-language "how sure" indicator, a last-updated timestamp, the two or three main drivers behind it, and an explicit rule for when it shows nothing at all.

#### F1. Memory — "can you recall this right now?"

**What it is.** The probability the student can retrieve a fact they've studied, right now. Per concept it's read almost directly off FSRS; per section and whole-exam it's an aggregate across the concepts in scope.

**The science and how it works.** This is the one score that's close to free and close to trustworthy, because FSRS already models retrievability per card (`memory_state`: stability and difficulty give predicted recall at a given elapsed time). Aggregating card-level retrievability up the `concepts` graph gives section- and exam-level Memory. Its range is the narrowest of the three because it's the best-validated quantity we have. The caveat belongs on the dashboard itself: Memory is *recall*, not application, so on its own it overstates exam readiness, which is the entire reason Performance exists as a separate number.

**Success.** Memory tracks actual delayed recall on held-out items within its stated range.

#### F2. Performance — "can you answer a new, exam-style question that uses this?"

**What it is.** The probability the student gets a *novel* application or passage-style item right, including concepts they've never been tested on in application form. This is the score that matches what the MCAT actually grades, and it's the hard one to estimate.

**The science, and why its range is wide.** Recall does not equal transfer. Pan & Rickard put retrieval-practice transfer at d ≈ 0.40 overall, dropping to d ≈ 0.28 when the practiced and tested responses don't overlap, and collapsing toward zero when initial recall is weak. So Performance is estimated as Memory discounted by a transfer factor, where the discount grows when recall is weak, the item format differs from what was practiced, and the task is application rather than near-recall. For concepts with direct application-item history we can also use a knowledge-tracing-style estimate, but the honest ceiling there is about 0.70–0.82 AUC for next-item correctness, and for a concept the student has *never* attempted in application form the cold-start accuracy is near chance unless prerequisite signal helps. That's why Performance always carries a visibly wider range than Memory, and why it leans on the prerequisite graph (Group D) to set priors for untested concepts.

**How it works on the architecture.** A prediction service reads FSRS retrievability, application-item history from the revlog, and prerequisite mastery from the `concepts` graph, and returns a calibrated probability with an interval. We validate calibration (expected calibration error, reliability diagrams) on held-out application items before surfacing it, not just AUC. The "unaided next-item correctness" from the tutor and practice flows is the ground truth we calibrate against.

**Success.** Predicted Performance is calibrated (a stated "70%" means roughly 70% observed) on held-out application items, and the interval widens appropriately for untested concepts.

#### F3. Readiness — "what would you score today, and how sure are we?"

**What it is.** A projected MCAT score on the real 472–528 scale, per section and overall, with a range and a confidence note. This absorbs the earlier readiness projection.

**The science / evidence, which is thin and we say so.** There is one peer-reviewed MCAT-prediction study we found (Chen & Corridon 2020), n = 19, where the median of a student's practice exams was the best single predictor (r = 0.92) and students tended to underperform their practice *maximum* on test day. The "FL4/FL5 are most representative," "average your last two," and "less reliable above 515" points are prep-community consensus, not peer-reviewed. Third-party deflation offsets trace to a self-reported Reddit/SDN dataset (Joel Harris), where Kaplan ≈ +10 is the best-corroborated figure and the "Blueprint +2 to +7" number is a vendor repackaging of Harris's original NextStep figures. The real MCAT also carries a built-in standard error of measurement of about ±2 points, so a single-point projection is false precision by construction. Prediction is worst at the top of the scale, where there are few data points above 515.

**How it works, and where it abstains.** Readiness blends AAMC full-length scores (weighted most heavily), our internal Performance aggregate, and third-party scores adjusted by documented offsets and labeled as estimates. It widens its range toward the extremes and abstains (a hedged "not enough signal yet" rather than a number) above ~515 until at least two or three AAMC-style full-lengths exist. Because published offsets are weak, the app builds its own predicted-vs-actual calibration dataset from users who later report real scores, and improves over time.

**Success.** Calibration improves as the dataset grows (stated ranges contain actual scores at the stated rate) and beats the published third-party offsets on our own users.

#### F4. The shared display contract and the give-up rule

**What it is.** Every score, at every granularity, renders the same seven things: point estimate, likely range, coverage %, a "how sure" chip (Low/Medium/High), last-updated timestamp, the top two or three drivers, and an abstention state. And there's a hard rule: when the data is insufficient, the app shows nothing rather than a shaky number.

**The science.** Two convergent literatures drive this. First, uncertainty communication: verbal probability terms are read inconsistently (Irwin & Mandel found roughly a quarter of experts and over half of non-experts translate "likely/unlikely" incoherently), and non-experts do better with explicit ranges than with point estimates, so we always show a plain-language range ("likely 508–512"), not a lone number or a raw "95% CI." Second, selective prediction / the "reject option" (Chow; El-Yaniv & Wiener; Geifman & El-Yaniv): a model that can decline to answer below a confidence threshold beats one forced to always answer in high-stakes settings, and the assessment analog is the CAT standard-error stopping rule. Showing the drivers (explainability) improves appropriate trust, and prominent ranges counter the complacency a single confident number breeds.

**The give-up rule, concretely.** A score is withheld when any of three conditions fail: coverage is too low (too little of the concept or section has been assessed), the credible interval is too wide, or calibration for that estimate is inadequate. In those cases the app shows an honest "keep practicing, not enough data yet" state naming what's missing, rather than a low-confidence figure. The thresholds (interval width, coverage %) start conservative and get tuned; over-abstaining frustrates users, so the coverage/reliability trade-off is a deliberate dial, not a fixed constant.

**How it works on the architecture.** All three scores are read-models computed in a service layer and surfaced through the shared web UI; the abstention check is a single gate in that service applied uniformly before any score is returned, so the "show nothing" rule can't be bypassed by an individual screen.

**Success.** Users interpret the ranges correctly in testing, and withheld-score states line up with genuinely unreliable predictions (when we do show a score, it's calibrated).

---

## 7. Success metrics

**North-star:** projected-and-then-actual MCAT score gain per study hour, for our users, versus their baseline trajectory.

**Primary learning metrics (what we optimize):**
- Calibrated Performance: unaided correctness on novel application items (the transfer proxy that matches what the MCAT grades).
- Concepts mature and test-ready (the Memory aggregate: FSRS stability by concept).
- Readiness-vs-actual calibration (do our stated ranges actually contain real scores).
- Actual AAMC full-length trajectory.

**Guardrail metrics (must not regress):**
- Daily/weekly active use and 4-week retention (adoption is a hard constraint).
- Total review load per unit of mastery (efficiency; watch for the FSRS relearning tax).
- AI item defect and residual-hallucination rate (must stay at or below the human baseline).

**Explicit anti-metrics (we do NOT optimize, and we hide or de-weight them):**
- Total cards created, total reviews done, raw hours, and streak length. These are the proxies the research says drift away from learning.

---

## 8. Technical architecture summary

The point of reviewing the Anki architecture was to make sure this is buildable on it. It is, and the split is fairly clean.

**What lives in the shared Rust core (`rslib/`), and therefore works on desktop and mobile at once:**
- Scheduling changes: the test-date governor (A2), interleaving defaults (A3), and trickle-down credit + mastery gating (D2) are all edits under `scheduler/` (`answering/`, `queue/`, `fsrs/`). Because AnkiDroid consumes the same core through the ~150-line JNI shim (`rslib-bridge`, calling the same `run_service_method` dispatcher), we implement once.
- New data model: the knowledge graph (`concepts`, `concept_edges`, `card_concepts`) and card lineage go in as new SQLite tables via a schema migration (`storage/upgrades/`, current schema v18), with matching messages in `proto/anki/*.proto` so Rust, Python, TS, and Kotlin all see them after codegen.
- Per-card state we get for free: FSRS memory state and the `custom_data` JSON field already on every card give us somewhere to hang lineage and mastery flags without a migration for the lightweight cases.

**What lives in a service/app layer (because it needs network and orchestration the core shouldn't do):**
- The grounded AI pipeline (C1), the tutor (C2), leech-repair generation (C3), the adaptive placement assessment (D3), and the three-score analytics/prediction (F). These retrieve from the vetted corpus or from read-models, do their orchestration (LLM calls, IRT item selection, calibration), and write results back through normal RPCs (`add_note` for content; per-concept mastery plus FSRS seeding for placement). Keeping this out of the Rust core respects the existing design: the core owns data and scheduling, not external I/O beyond sync.

**What lives in the shared web UI (`ts/`, Svelte 5, rendered in the embedded browser on desktop and via the NanoHTTPD-served pages on Android):**
- The scores dashboard (E1) and the three-score views (F), the placement flow (D3), the coverage report (B4), and mastery/graph views. New RPCs that the web needs must be added to the media-server allow-list (`mediasrv.py:exposed_backend_list`), since the web frontend can only reach allow-listed methods.

**Cross-cutting note on extension points.** The clean way to add backend behavior is: add the message + RPC to the right `.proto`, run a full build so codegen regenerates all four language bindings and the dispatcher, implement the trait method in the matching `service*.rs`, and (if the web needs it) add it to the allow-list. Service/method indices are positional and shared across languages, so we only ever append RPCs, never reorder them. Mutations run off the UI thread via the operations framework and must return the right `OpChanges` flags to refresh the UI.

**Mobile-specific realities to respect (from the Android notes):** cards are pre-rendered in Kotlin on AnkiDroid (only the richer Svelte pages use the local server), there are no add-ons on mobile, and version coupling between the app and the backend AAR is strict (mismatched proto indices fail silently). None of this blocks us; it just means the mobile client stays a consumer of the shared core and the synced content.

---

## 9. Roadmap and phasing

Phasing follows the research staging: prove the cheap, well-evidenced wins first, then layer on the harder, more experimental pieces.

**Phase 1 — MVP: a better-scheduled, application-first Anki.**
- FSRS at workload-efficient retention (A1).
- Interleaving by default (A3).
- Application-form item types + error-driven minting (B1, B2).
- Opt-in premade coverage deck + basic AAMC coverage checker (B4).
- Learning-aligned dashboard (E1) leading with the Memory score (F1), which FSRS gives us almost for free.
- Basic concept tagging (a lightweight precursor to the full graph).
- *Rationale:* every item here rests on strong or at least directional evidence and requires no unproven parameter. This alone should beat mindless deck-grinding.

**Phase 2 — Grounded AI and the knowledge graph.**
- Grounded generation pipeline with rule-based flaw check + human review (C1).
- Leech repair with early intervention (C3).
- Full prerequisite knowledge graph + per-concept mastery (D1).
- Mastery gating + trickle-down credit (D2).
- Card metamorphosis, add-then-fade version (B3).
- Adaptive placement assessment seeding per-concept mastery (D3), once the graph exists.

**Phase 3 — Tutoring, deadline scheduling, and adoption (the experiments).**
- AI tutor grounded in student state (C2).
- Test-date governor (A2), behind an A/B with a kill criterion.
- Adoption mechanics: streak-freeze + reward-the-hard-thing (E2/E3), behind an A/B with an adherence guardrail.
- Performance score with calibration on application items (F2).

**Phase 4 — Prediction and refinement.**
- Readiness projection + proprietary calibration dataset (F3), and the full display and abstention contract across all three scores (F4).
- Tuning of trickle-down discounts, gating thresholds, and the deadline curve from real outcome data.

---

## 10. Risks, open questions, and the experiments that resolve them

A recurring discipline in this project is refusing to turn lab effect sizes into shipped parameters. These are the bets we're explicitly testing rather than assuming.

- **Does the deadline governor actually help (A2)?** Unknown; no RCT exists. A/B against flat FSRS; kill if total workload rises or practice scores fall.
- **Does anti-streak gamification keep people engaged (E2/E3)?** Unknown; it's a hypothesis. A/B against a gentle streak; revert if adherence drops. This is the highest-variance bet in the product because adoption is existential.
- **Does removing the "finish the deck" backbone hurt cold-start (B1/B4)?** Possible. That's why the premade deck stays opt-in and minting is additive; we watch week-1 retention in a no-deck vs with-deck comparison.
- **Are trickle-down credit and mastery gating tuned right (D2)?** The FIRe formula is a practitioner heuristic. We tune the discount and thresholds empirically against review load and unlocked-item success.
- **Can grounded AI actually hold defect/hallucination at or below human levels at scale (C1)?** Grounding reduces but never eliminates hallucination. We set a monitoring target and keep human review in the loop; if residual defects exceed the human baseline, generation stays gated harder.
- **Does the app beat the *disciplined* top-scorer workflow, not just the lazy one?** This is the real prize and it's genuinely uncertain. It's answered only by outcome data (actual AAMC trajectories) once Phases 1–2 are live.
- **Does placement credit mastery correctly (D3)?** A single correct answer is a weak mastery signal. We require application-level and spaced confirmation before skipping content, and audit credited concepts against delayed checks; if more than ~10% fail, we raise the bar.
- **Can we estimate Performance for untested concepts honestly (F2)?** Cold-start prediction is near chance and knowledge tracing tops out around 0.70–0.82 AUC. We validate calibration before showing the number, and widen or withhold the range when signal is thin, rather than projecting false confidence.

**Evidence honesty, carried forward:** all Anki→exam correlations are correlational, single-site, and non-MCAT; Khanmigo, Duolingo, and Math Academy figures are company-reported; the MCAT-prediction base is one n=19 study plus self-reported community data; Omar's hallucination rate is adversarial worst-case and Asgari's is a summarization figure from direct prompting, not RAG. We build features to test these, not to trust them.

---

## 11. Out of scope / non-goals

- Content not tied to the MCAT.
- Live instructional classes or human tutoring.
- Being a general-purpose flashcard app; every design choice optimizes for the MCAT specifically.
- Ungrounded AI generation of any kind (a hard non-goal, per Principle 4).
- K–12 or early-childhood-specific features, except where a finding generalizes.
- Optimizing engagement metrics (DAU, session length, streaks) as ends in themselves.
- "Deck completion" as a user goal or a surfaced metric.

---

## Appendix — Feature-to-evidence map (quick reference)

| Feature | Primary evidence | Strength / caveat |
|---|---|---|
| A1 FSRS at efficient retention | FSRS workload analysis (U-shaped); Bjork spacing = the gap | Simulation on community data; direction solid |
| A2 Test-date governor | Cepeda 2008 (gap shrinks near deadline); FSRS is deadline-agnostic | Analogy, not proof; ships as A/B |
| A3 Interleaving default | Rohrer 2020 RCT (61 vs 38, d=0.83); Cepeda 2006 | Strong (large classroom RCT) |
| B1 Error-driven minting | Jack Westin; Pan & Rickard congruency; Deng 2015 | Good but not novel; make it additive |
| B2 Application items | Pan & Rickard (0.28→0.58, +0.23); Karpicke & Blunt; Willingham | Strong direction; Blunt magnitude contested (Mayrhofer 2023) |
| B3 Metamorphosis (add-then-fade) | Expertise reversal (correctly scoped); New Theory of Disuse | Corrects a misapplication; don't delete recall |
| B4 Coverage deck + checker | Coverage-gap risk; SDN existence proofs | Answers cold-start/coverage risk |
| C1 Grounded generation | Omar 2025 (adversarial); Asgari 2025 (summarization, not RAG); Chelli 2024; Doughty 2024 | Numbers correctly contextualized; grounding ≠ zero |
| C2 State-grounded tutor | Khanmigo +6.1% next-item; +2.7% prerequisites | Company A/B; ~15% adoption ceiling drives design |
| C3 Leech repair | Anki leech manual; Wozniak (interference); Sweller/cognitive load | Reframes leeches as a teaching trigger |
| D1 Knowledge graph | Willingham; Bjork prerequisites; Math Academy; Bloom mastery | Direction strong; MA speed claims are marketing |
| D2 Gating + trickle-down | FIRe; Khanmigo prerequisites; Bjork; Pan & Rickard (transfer≈0 if unready) | Heuristic; tune empirically |
| D3 Placement assessment | ALEKS eval (AUROC 0.89, vendor); IRT/CAT efficiency; Soderstrom & Bjork; Koriat & Bjork | Efficient; never credit mastery on one answer |
| E1 Learning-aligned dashboard | Wright State (maturity); Soderstrom & Bjork; Khan metric; LAD reviews | Correlational; show drivers, not bare numbers |
| E2/E3 Adoption mechanics | Duolingo retention (vendor); Shortt 2023; overjustification | Highest-variance bet; A/B with adherence guardrail |
| F1 Memory score | FSRS retrievability (per-card) | Best-validated of the three; narrow range |
| F2 Performance score | Pan & Rickard (transfer d=0.28–0.40); knowledge tracing AUC ~0.70–0.82; cold-start ≈ chance | Hard to estimate; wide range; calibrate before showing |
| F3 Readiness projection | Chen & Corridon 2020 (n=19); Harris offsets (self-report); ±2 SEM | Thin evidence; abstain above ~515; build own calibration |
| F4 Display + give-up rule | Uncertainty comms (Irwin & Mandel); selective prediction (Chow; El-Yaniv) | Show range + drivers; withhold when data thin |
