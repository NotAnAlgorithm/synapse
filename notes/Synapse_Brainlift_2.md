# Synapse — Brainlift

**Owners:** e

Working master doc for the thinking behind Synapse, an MCAT study app built on the Anki core and grounded in learning science. This is the second major version. It folds in two rounds of fresh-evidence research: a re-adjudication of the original brainlift against its red-team counter, and a follow-up study on placement testing, performance prediction, and score display. Where the earlier version over-reached, this one says so, because the disagreements between sources turned out to be where most of the useful ideas came from.

---

## Purpose

Figure out every defensible way to modify and optimize Anki for MCAT studying, grounded in learning and cognitive science, real studies, learning-app builders, instructors, and students, so we can build features that actually raise MCAT scores per hour studied.

### In scope
- Pedagogical techniques and principles we can turn into app features.
- Going past flashcards into a whole MCAT study ecosystem (practice, knowledge graph, grounded AI, analytics).
- AI features that improve learning without inventing facts.
- Mechanisms of and improvements to spaced repetition.
- MCAT score prediction and student-performance modeling.

### Out of scope
- Content not tied to the MCAT.
- Live instructional classes.
- K-12 / early-childhood-specific findings, unless they generalize.

### How to read this
Claims are graded by evidence quality, and I flag when something is correlational, single-site, vendor-authored, simulation-only, or an adversarial worst case. Tensions between sources are kept on purpose rather than smoothed over. The DOK 4 SPOVs are the spiky product bets; the DOK 3 insights are the reasoning underneath them; each insight leans on several sources rather than one.

---

## DOK 4 — Spiky Points of View

Seven bets, deliberately spread across seven different axes so they don't collapse into one idea: **what cards exist** (1), **how they're scheduled** (2), **how they evolve** (3), **how AI builds and repairs them** (4), **how the student is motivated** (5), **how learning is measured** (6), and **how students are placed when they arrive** (7). Together they describe an app that is recognizably not "Anki with a chatbot bolted on."

The two I'd defend as most genuinely novel-yet-grounded are **SPOV 6** (three separate scores plus the discipline to show nothing when we don't know) and **SPOV 2** (deadline-aware scheduling, but with the retention lever pointed the opposite way from where the first draft pointed it).

### SPOV 1 — Mint cards from mistakes, not from a mega-deck, but keep a coverage net.

MCAT Anki culture treats grinding a 5,000-card premade deck to completion as the goal. That runs the causal arrow backwards: memorize first, then hope it transfers. The only Anki statistic that reliably tracks exam outcomes is percent of cards *mature*, not card count, review count, retention rate, or ease. So the app should mint cards on demand from missed practice questions, link each card to the item that spawned it, and show "concepts mature and test-ready," never "cards remaining."

- **What changed from v1:** the original claim was that maturity is the *only* thing that predicts anything, and that we should ship *zero* premade content. Both were too strong. A second cohort (UNLV) found mature-card count, study hours, and unsuspended-card count all correlated with scores; they lose independent significance only because they're collinear with maturity. And premade decks are a proven, zero-setup, high-coverage tool with real 508→520 existence proofs. So the corrected bet keeps error-driven minting as the *default* path but keeps an *opt-in* premade deck plus a coverage checker against the AAMC content outline.
- **Supporting research:** Wright State and UNLV cohorts (maturity is the most robust correlate); Wothe (daily use tracked Step 1); Jack Westin ("Anki prevents re-forgetting"; tie cards to passage mistakes); Pan & Rickard (bare-recall transfer d≈0.28).
- **Tension / steelman:** error-only minting has a real coverage-gap risk. You never make a card for a high-yield topic you happen not to have been tested on, and Psych/Soc is broad enough that this bites. The coverage checker and diagnostic seeding are load-bearing, not optional. Also, all the maturity evidence is correlational, single-institution, and not from the MCAT.

### SPOV 2 — Schedule backward from test day, but raise retrieval *late*, don't lower it *early*.

FSRS optimizes for indefinite retention, not for peak recall on one specific Saturday, and almost no tool schedules against a deadline at all. That gap is real. The fix is a test-date governor that compresses intervals and lifts effective retrievability in the final couple of weeks so recall peaks on exam day.

- **What changed from v1, and this is the biggest correction in the whole doc:** the original SPOV said to *lower* desired retention early to manufacture Bjork-style desirable difficulty, tolerating more forgetting up front. That's a category error on two counts. First, the desirable-difficulty lever is the *spacing gap*, which FSRS already operationalizes by scheduling a review right when predicted recall decays to the target. Lowering the desired-retention *parameter* is a different knob. Second, the FSRS workload curve is U-shaped: push retention too low and total work *rises*, because cards lapse and you pay the relearning tax. There's also a gap between the parameter and reality (a 0.90 target yields ~0.95 true average retrievability, since reviews fire at the due moment). So "lower early" doesn't even do what it claimed. The defensible move is the opposite: keep FSRS near its workload-optimal retention (~0.85–0.90) and raise retrievability *late*.
- **Supporting research:** Cepeda et al. (optimal gap shrinks as the horizon to the test shrinks); FSRS developer docs (U-shaped workload; the deadline knob doesn't exist natively); Bjork & Bjork (storage gain is greatest at low retrieval strength, achieved via the gap).
- **Tension / steelman:** Cepeda tested the gap between *two* study sessions at a fixed test delay, not a multi-review SRS curve, so it motivates the direction but doesn't prove a specific curve. There is no RCT of a deadline-aware retention curve anywhere. This ships behind an A/B against flat FSRS, and it dies if total review load (including relearning) rises or practice scores fall.

### SPOV 3 — A recall card is a chrysalis, not a corpse. Grow application on top; don't kill the recall.

The MCAT almost never asks "what is X." It asks you to wield X inside a novel experiment, and CARS is pure reasoning. Bare recall trains the low-transfer regime. So once a fact is stable, the app should stop leaning on the bare-recall version and promote the concept into interleaved, passage-embedded application items, and it should grade mastery on the *application* form.

- **What changed from v1:** the original justified *expiring* (retiring) recall cards at mastery using the expertise-reversal effect. That's a misapplication. Expertise reversal is about *instructional guidance* (worked examples, redundant explanations) becoming useless or harmful for experts. Retrieval practice is not instructional guidance; it's the desirable difficulty itself, and nothing in expertise reversal says stop retrieving mastered facts. Worse, the New Theory of Disuse says retrieval strength decays without use, so a "mastered" fact whose card you retired will quietly rot before a fixed-date exam. And some MCAT content genuinely is recall (amino acids, hormones, equations, and the near-identical term pairs Psych/Soc loves to exploit). So the corrected bet: *add* application items on top and let FSRS stretch the recall interval on its own (a stable card is already cheap to maintain), retiring the recall card only after application-level mastery, and keep light maintenance retrieval before the exam.
- **Supporting research:** Pan & Rickard (response congruency; elaborated retrieval +d≈0.23); Karpicke & Blunt (retrieval reaches inference); Rohrer (interleaving forces strategy selection, 61% vs 38%); Bjork & Bjork (decay without retrieval); expertise-reversal literature (correctly scoped to guidance).
- **Tension / steelman:** there's an internal tension with SPOV 2, which says keep retrievability up before the deadline. Retiring a mastered card conflicts with that, which is exactly why we fade rather than delete and schedule maintenance reviews. Also, the Karpicke & Blunt d≈1.50 headline is inflated. A 2023 preregistered replication (Mayrhofer et al.) showed a big chunk of it was a memorization-time artifact, so we cite the *direction* (retrieval reaches higher-order learning), not the size.

### SPOV 4 — AI is a grounded transformer, never an oracle, and its best job is re-teaching, not mass-producing.

The current wave of AI flashcard and quiz generators spins content out of a model's parametric memory, which is the ungrounded regime where medical hallucination and fabricated citations run wild. Source-grounded transformation is far safer. So AI is sandboxed to transforming vetted content (summarize *this* AAMC-aligned passage into atomic cards, write distractors from *this* verified key, explain *this* correct solution), with a citation to the source on every item and a quality gate before anything reaches a student. The flagship use is leech repair: at the first couple of lapses, atomize the card, find the interfering sibling, and run a 60-second worked-example micro-lesson before rescheduling.

- **What changed from v1:** the conclusion (ground the AI) is right and holds up, but the original quantitative case mixed incommensurable numbers. The scary "up to ~83%" figure is Omar et al., an *adversarial* study where each prompt had a fabricated detail planted, so it's a worst case, not a base rate, and prompting only cut it to ~44%. The reassuring "~1.47%" figure is Asgari et al., but it's a *summarization* task done with *direct* GPT-4 prompting, not RAG, so it isn't evidence that grounding is safe. The honest story: grounding sharply reduces hallucination but never to zero (best clinical RAG still sits around 5–6%), so we set a residual-hallucination monitoring target instead of pretending it's solved.
- **Supporting research:** Omar et al. (adversarial ≤82%); Asgari et al. (1.47%, summarization, direct-prompt); Chelli et al. (fabricated references, GPT-4 28.6% / Bard 91.4%); Doughty et al. (AI MCQs match human quality on average but carry ~4–5% defect rates vs ~1%, and an LLM is a worse item-quality judge than a rule-based checker); Anki "Leeches" and Wozniak (interference is the chief cause of forgetting).
- **Tension / steelman:** strict grounding is a real operational cost. It needs a vetted source pipeline, it's slower, and it covers less than free-form generation. The bet only holds if the grounding and QA gate are actually enforced, since one confidently-wrong card on a scored exam is worse than a smaller, slower, trustworthy library. Grounding-only is also a sound principle rather than a moat, since some existing tools already do grounded generation.

### SPOV 5 — Reward the hard rep, but don't burn down the streak.

Consumer edtech reflexively copies the streak/XP/league loop, and those are engagement proxies that can drift from learning. Speed-running an easy lesson to protect a streak "activates the retention mechanism without the learning," and restudy even inflates confidence while worsening retention. So points should accrue for desirable difficulty (nailing a card after a long gap, resurrecting a lapsed card, answering delayed application items unaided) and not for binge-cramming young cards or padding review counts.

- **What changed from v1:** the original wanted to strip streaks and easy-rep rewards wholesale ("anti-streak"). That's the highest-variance bet in the project and it has no supporting outcome evidence. Streaks and loss-aversion mechanics are among the most robust *adherence* drivers known, and an app nobody opens teaches nothing. Even Khanmigo, an excellent grounded tutor, sees only ~15% of eligible students engage. So the corrected bet keeps low-pressure adherence mechanics (streak-freeze and forgiveness so one missed day during crunch doesn't wipe progress) alongside the reward-the-hard-thing design, and A/B-tests the anti-streak idea against a gentle streak with an adherence guardrail.
- **Supporting research:** Bjork & Bjork (performance ≠ learning; illusion of competence); Roediger & Karpicke (restudy inflates confidence, worsens retention); Duolingo analyses (streak-protection ≠ learning, but streaks drive large retention gains, vendor-reported); Shortt et al. (Duolingo gamification mostly helped lower-order vocabulary); Khan Academy (~15% adoption ceiling).
- **Tension / steelman:** the whole thing is a trade between learning purity and adherence, and the anti-streak prescription is a hypothesis with zero outcome data. Adherence (daily/weekly active use, 4-week retention) gates this feature, not learning purity alone. If anti-streak reduces engagement, it reverts.

### SPOV 6 — Measure three things, not one: Memory, Performance, Readiness, and shut up when you don't know.

Every flashcard app collapses progress into one number that feels like "mastery." That's the core measurement error, because three genuinely different things are hiding inside it. **Memory** is the chance you can recall a fact right now (essentially FSRS retrievability, which we already compute). **Performance** is the chance you can answer a new, exam-style application question that uses the fact, including questions you've never seen. **Readiness** is your projected MCAT score. These come apart in practice: a student can be high-Memory and low-Performance, and showing that gap is more honest than any single mastery bar. On top of that, the system must *abstain*, showing nothing rather than a confident-but-wrong number, whenever it lacks the data to estimate honestly.

- **Why this is new:** neither the original brainlift nor its counter separated these three constructs, and the abstention discipline is the direct operational counter to the illusion of competence. It's the measurement expression of "performance ≠ learning."
- **Supporting research:** Pan & Rickard (recall and transfer are different quantities; transfer is d≈0.28–0.58 depending on congruency and collapses when recall is weak); knowledge-tracing benchmarks (next-item-correctness prediction tops out around 0.70–0.82 AUC, with deep models beating simple baselines by only ~1–3 points); cold-start studies (on a never-seen skill, prediction is near chance, ~0.49–0.65 AUC); selective prediction / reject-option literature (abstaining beats showing low-confidence predictions in high-stakes settings); uncertainty-communication research (verbal probabilities are interpreted inconsistently; ranges help non-experts); Soderstrom & Bjork (performance is an unreliable index of learning); MCAT prediction is least reliable at 515+.
- **Tension / steelman:** predicting Performance for untested concepts is genuinely hard (near-chance at cold start), so its uncertainty must be strictly wider than Memory's, and prerequisite-graph inference (used to set priors) is largely vendor/simulation-validated. Over-abstaining also frustrates users, so the show-nothing threshold has to be tuned, not maxed.

### SPOV 7 — Meet students where they are, but make them prove it.

Many students arrive having already done real content review or practice elsewhere. Forcing them through everything from scratch wastes their scarcest resource. So an optional adaptive placement assessment should measure what they already know and let them skip redundant reviews. But mastery must never be credited on a single in-the-moment correct answer.

- **Why this is here:** it fills the "diagnostic seeding" the original brainlift only gestured at, and it directly inherits SPOV 6's discipline about not trusting a single lucky hit.
- **Supporting research:** knowledge-space theory / ALEKS (adaptive placement classifies known vs unknown topics with AUROC ≈ 0.89, though the evidence is vendor-authored); computerized adaptive testing and IRT (roughly 50% fewer items for equal or better measurement precision, with principled stopping rules); Soderstrom & Bjork and Koriat & Bjork (a single correct response, especially when the material felt fluent, is an unreliable mastery signal); retrieval-practice research (durable retention needs repeated, spaced, successful retrieval, not one recall); ALEKS data showing even "Known" topics are missed ~17% of the time, i.e., slips are real.
- **Tension / steelman:** the efficiency evidence is mostly vendor or simulation-based, and there's essentially no direct evidence that placement reduces redundant study in *MCAT* prep specifically. So placement credits *partial* mastery (seed FSRS with a shortened but nonzero interval), confirms with an application-level item plus a spaced delayed check, and tolerates slips before ever fully retiring reviews. We should treat our own deployment as the experiment that generates the missing evidence.

---

## DOK 3 — Insights

Grouped by theme. Each leans on several sources, and I keep the tension where one exists.

### Theme A — The retention-application gap is the whole game

**Insight A1 — Transfer is the ceiling, and it's a property of card *form*, not of flashcards themselves.** The MCAT rewards applying knowledge to novel passages, and CARS has nothing to memorize. Retrieval practice does transfer to inference and application, but only moderately and conditionally: about d = 0.40 overall, falling to d = 0.28 when the practiced and tested responses don't overlap, rising to d = 0.58 when they do, plus roughly 0.23 more for elaborated retrieval, and it drops toward zero when initial recall is weak. Bare cloze cards sit in that weak end. The real lever isn't "flashcards vs practice tests," it's moving cards out of bare recall into elaborated, application-framed retrieval.
- *Sources:* Pan & Rickard (2018); Karpicke & Blunt (2011); Willingham (2007, reasoning is domain-bound); MCAT/CARS structure.
- *Tension:* two "elaborat-" ideas get confused. Elaborative *study* (concept mapping used instead of testing) loses to retrieval, per Karpicke & Blunt; elaborated *retrieval* (enriching the retrieval act with explanation and higher-Bloom prompts) *adds* transfer, per Pan & Rickard. They agree once you separate them. Also, the Karpicke & Blunt d≈1.50 is inflated by a memorization-time confound (Mayrhofer et al. 2023), so use the direction, not the number.
- *→ SPOVs 1, 3.*

**Insight A2 — Memory and Performance are different quantities, and predicting Performance is the hard part.** Because recall strength predicts transfer only weakly, knowing a student can recall a fact tells you surprisingly little about whether they can use it on a novel item. Learner-modeling methods that predict next-item correctness plateau around 0.70–0.82 AUC, and on a concept the student has never attempted, prediction is near chance until a few attempts accumulate. So Performance has to be estimated as recall discounted by a transfer factor, carried with much wider uncertainty than Memory, and leaned on prerequisite structure only for priors.
- *Sources:* Pan & Rickard (2018); pyKT / Gervet et al. knowledge-tracing benchmarks; cold-start studies (Zhang et al. 2021); Math Academy FIRe and ALEKS knowledge spaces (prerequisite inference, vendor/simulation).
- *→ SPOV 6.*

### Theme B — Measure the right thing, and admit uncertainty

**Insight B1 — Percent mature is the most robust Anki correlate, but it is not the *only* predictor, and it's correlational.** Across cohorts, maturity tracks exam performance better than raw volume, but it isn't alone and the data can't establish causation. In one cohort, mature-card count was the only independent predictor for a single course (explaining ~36% of variance there); in another, mature cards, study hours, and unsuspended cards all correlated with the outcome and were collinear; a third found daily use tracked Step 1 but not Step 2. The old "1,500 cards per point" line is a garble of a real but weak correlational finding.
- *Sources:* Wright State / Gilbert et al. (2023); UNLV / Winter et al. (2025); Wothe et al. (2023); Deng et al. (2015, ~1,700 cards *or* ~445 practice questions per Step 1 point, correlational, pre-AnKing).
- *Tension:* the original "only maturity predicts anything" overstated a single-course, single-site result. All of this is correlational, small-sample, and none of it is from the MCAT, so maturity is the best north-star we have but should be shown honestly, not as proof of causation.
- *→ SPOVs 1, 6.*

**Insight B2 — The honest system shows a range and knows when to stay silent.** Confidence, fluency, and a single tidy number all overstate learning, so a progress metric that hides uncertainty will mislead. The fix, borrowed from selective prediction and from adaptive-testing stopping rules, is to show a point estimate with an explicit range, a plain-language "how sure" label, the drivers behind it, and nothing at all when the data or the calibration isn't there yet.
- *Sources:* selective prediction / reject-option (Chow; El-Yaniv & Wiener; Geifman & El-Yaniv); uncertainty-communication research (verbal probabilities interpreted inconsistently; predictive intervals help non-experts); learning-analytics dashboard reviews (weak evidence dashboards help; explainability improves appropriate action); Soderstrom & Bjork; Koriat & Bjork.
- *→ SPOV 6.*

### Theme C — Scheduling is a deadline problem, but not the way we first thought

**Insight C1 — Deadline-awareness is a real gap in FSRS, but "lower retention early" is a category error.** FSRS optimizes for indefinite retention and exposes a desired-retention knob, but that knob is not the desirable-difficulty lever, and turning it down doesn't buy free difficulty. The desirable difficulty is the spacing gap, which FSRS already builds in; the workload curve is U-shaped, so a too-low target *raises* total work via relearning; and the parameter understates true average retrievability anyway. The genuine opportunity is to make scheduling deadline-aware by lifting retrievability late, which is the opposite posture from the first draft.
- *Sources:* FSRS developer docs / benchmark (U-shaped workload; ~20–30% fewer reviews than SM-2 for equal retention, simulation-derived; deadline-agnostic by design); Cepeda et al. (optimal gap shrinks as the test nears); Bjork & Bjork (storage gain greatest at low retrieval strength, via the gap).
- *Tension:* Cepeda is a two-session-gap result, not a validated multi-review deadline curve, and no such curve has been trialed. This stays an experiment with a workload-and-scores kill criterion.
- *→ SPOV 2.*

### Theme D — Failure modes are diagnostic signal

**Insight D1 — Leeches and ease-hell are card-quality problems, best fixed by early re-teaching, not by rescheduling harder.** A chronically failing card usually means the card is non-atomic, interfering with a sibling, or simply not understood. SM-2 responds by shrinking intervals (the ease spiral); Anki flags a leech only after 8 lapses and blames the user, while its own manual attributes leeches to bad or interfering cards. So the highest-leverage move is to detect the failing card early, atomize or disambiguate it, and re-teach with a worked example, which is exactly what novices need under cognitive-load theory.
- *Sources:* Anki "Leeches" manual (interference; 8-lapse default); Wozniak (interference is "probably the single greatest cause of forgetting"); Sweller and Kirschner/Sweller/Clark (worked examples lower load for novices); SM-2 ease mechanics.
- *→ SPOV 4.*

### Theme E — AI: grounding is the bright line, but the numbers were oversold

**Insight E1 — Ground the AI or don't ship it, but the hallucination numbers are non-comparable and grounding never reaches zero.** The scary and reassuring hallucination figures measure different tasks under different conditions, so stacking them into one "ground first" argument mixes incommensurable numbers even though the conclusion is right. Ungrounded and adversarial generation fabricates freely and invents citations; grounded transformation is far safer but still errs a few percent of the time; and AI-written questions match human quality on average while carrying higher rates of specific defects. An LLM is also a worse judge of item quality than a rule-based checker, so the quality gate should be rules first.
- *Sources:* Omar et al. (adversarial ≤82%, prompting → ~44%); Asgari et al. (1.47%, summarization, direct-prompt not RAG); Chelli et al. (reference fabrication); best clinical RAG (~5–6% residual); Doughty et al. (MCQ defect ~4–5% vs ~1%; LLM judge ~79% vs rule-based ~91%).
- *Tension:* the honest headline is "ground the AI because the *direction* of all this evidence agrees," not "grounding gets you to 1.47%." Grounding also costs a vetted source pipeline and coverage.
- *→ SPOV 4.*

**Insight E2 — AI's real payoff comes from grounding in the student's *state*, and adoption is the binding constraint.** The measurable win from an AI tutor came specifically from feeding it the learner's history and surfacing their unmastered prerequisites, not from better chat, and the effect was measured as unaided next-item correctness, a genuine transfer metric. But the same program shows that even a good grounded tutor is used by only about 15% of eligible students, so the tutor has to be proactive and low-friction, appearing at the moment of a mistake rather than waiting to be opened.
- *Sources:* Khan Academy / Khanmigo (+6.1% next-item correctness overall; +3.4% from history; +2.7% from surfacing prerequisites; ~15% adoption; practice, not the chatbot, drives gains).
- *Tension:* the numbers are company-reported A/B tests, credible but not peer-reviewed.
- *→ SPOVs 4, 5, 7.*

### Theme F — Performance ≠ learning, the unifying error

**Insight F1 — Streaks, review counts, fluency, confidence, and single correct answers are the same trap in different costumes.** Every one of these is a performance or engagement proxy that can move opposite to durable learning. Fluent rereading feels like mastery, restudy inflates confidence while worsening retention, streak-protection activates retrieval without learning, and a single in-the-moment correct answer overstates what's actually been learned. The unifying lesson is to reward and display delayed, unaided retrieval and transfer, and to distrust any felt or counted proxy, which is why this one insight underwrites the measurement SPOV, the gamification SPOV, and the placement SPOV at once.
- *Sources:* Bjork & Bjork (storage vs retrieval; illusion of competence); Soderstrom & Bjork (performance is an unreliable index of learning); Roediger & Karpicke (restudy inflates confidence, worsens retention); Koriat & Bjork (foresight bias when material feels fluent); Duolingo critiques (engagement diverges from learning).
- *→ SPOVs 5, 6, 7.*

### Theme G — One deck or one path can't be right for everyone

**Insight G1 — Expertise reversal plus prerequisites make a static path structurally wrong; difficulty has to be titrated to per-concept mastery.** The same hard interleaved item is a desirable difficulty for a prepared student and damage for an unprepared one, because a difficulty is only desirable if the learner can meet it and transfer collapses when the prerequisites aren't there. Novices need worked examples and guidance that experts don't. So the app needs a prerequisite knowledge graph with per-concept mastery gating: worked examples first, retrieval and interleaving once a threshold is crossed, and implicit trickle-down credit so advanced practice maintains the basics.
- *Sources:* Kirschner/Sweller/Clark (2006, expertise reversal, scoped to *guidance*); Bjork & Bjork (desirable only with prerequisites); Pan & Rickard (transfer ≈ 0 when initial retrieval fails); Math Academy FIRe (prerequisite graph, trickle-down credit).
- *Tension:* expertise reversal is about guidance, not retrieval, so it does *not* justify expiring recall cards (see SPOV 3). And Math Academy's efficiency figures (e.g., "4x speed") are company marketing, not independent research, so we borrow the architecture, not the number.
- *→ SPOVs 3, 6, 7.*

### Theme H — Direction of flow: questions first, cards downstream

**Insight H1 — Cards belong downstream of missed practice, not upstream of it.** Practice is the diagnostic engine and cards are the retention patch. Interleaved practice forces the strategy-selection the exam tests, retrieval transfers best when it mirrors the criterion task, and AAMC full-lengths are the gold-standard readiness signal. So the highest-yield loop is practice, then error analysis by type, then an auto-generated elaborated card from the gap, then spaced review, then re-test in passage form. Most workflows run this backwards.
- *Sources:* Jack Westin; Rohrer et al. (interleaving 61% vs 38%); Pan & Rickard (response congruency); AAMC full-lengths; Deng et al. (practice questions bought points as efficiently as cards).
- *Tension:* this is established best practice among top scorers, not a novel idea, and pure error-driven minting risks coverage gaps, so minting is additive and paired with the coverage checker (SPOV 1).
- *→ SPOVs 1, 6.*

### Theme I — Meet students where they are

**Insight I1 — Placement can cut redundant work, but a single correct answer is a weak mastery signal.** Adaptive diagnostic assessment can locate what a student already knows efficiently and classify known versus unknown topics well, and adaptive testing needs far fewer items than a fixed test for the same precision. But crediting mastery off one correct response reintroduces the illusion of competence the whole app is built to fight, especially since even confidently "known" material is missed a meaningful fraction of the time. So placement should credit partial mastery, confirm with an application-level item and a spaced delayed check, tolerate slips, and seed FSRS with a shortened but nonzero interval rather than declaring a concept done.
- *Sources:* knowledge-space theory / ALEKS (placement classification AUROC ≈ 0.89, vendor-authored); computerized adaptive testing and IRT (roughly 50% fewer items; SE and PSER stopping rules); Soderstrom & Bjork and Koriat & Bjork (single-response unreliability, fluency-driven overconfidence); retrieval-practice research (durable retention needs repeated spaced success).
- *Tension:* the efficiency evidence is largely vendor or simulation-based, and there's no direct MCAT-specific evidence that placement reduces redundant study, so our deployment should A/B placement-credited against full-review cohorts.
- *→ SPOV 7.*

---

## Experts to Follow

**Piotr Woźniak (SuperMemo).** Invented computational spaced repetition (the SM family) that Anki's SM-2 and, indirectly, FSRS descend from, and co-authored the two-component (stability/retrievability) model that FSRS's DSR framing is built on. His minimum-information principle and "20 Rules of Formulating Knowledge" are the most actionable existing spec for card-quality enforcement at creation, and his line that interference is the single greatest cause of forgetting underwrites leech repair. Treat his wiki as a brilliant primary-practitioner source, not vetted literature, since he's abandoned peer review. supermemo.guru; @SuperMemoWoz.

**Jeffrey Karpicke (Purdue).** The most prolific modern investigator of retrieval-based learning: retrieval changes memory rather than just measuring it, beats elaborative study including on inference items, and works via retrieval-specific processes, and students hold strong metacognitive illusions about all of this. The evidentiary backbone for making every content unit recur as retrieval and for distrusting learner-reported confidence. Note the d≈1.50 concept-mapping result is contested (Mayrhofer et al. 2023). learninglab.psych.purdue.edu.

**Robert & Elizabeth Bjork (UCLA).** The learning-versus-performance distinction, the New Theory of Disuse (storage vs retrieval strength, with the biggest storage gains from effortful successful retrieval), desirable difficulties, and the illusion of competence. This is the theory that makes the spiky bets defensible, and also the theory that *corrected* two of them: it's why we don't confuse the retention parameter with the spacing gap, and why we don't retire mastered recall cards. bjorklab.psych.ucla.edu.

**Doug Rohrer (USF).** Interleaving improves STEM learning by forcing students to choose a strategy from the problem itself, shown in preregistered classroom RCTs (61% vs 38%, d=0.83), and spacing reduces overconfidence. The strongest real-world evidence that mixing problem and passage *types* transfers, and effectively a ready-made design spec for interleaving. uweb.cas.usf.edu/~drohrer.

**Justin Skycak (Math Academy).** Treats a STEM curriculum as a prerequisite knowledge graph with mastery learning, spaced repetition, interleaving, and Fractional Implicit Repetition (trickle-down credit to prerequisites), using regenerated non-memorizable questions. The closest existing product to what we're building and the direct source for the graph-plus-implicit-credit design. Practitioner/company perspective, so the efficiency figures are marketing, not research. justinmath.com.

**Others, lighter follows, now spanning the new areas too.** John Dunlosky (Kent State) for periodic "what actually works" technique reviews; Henry Roediger (WashU) as Karpicke's collaborator on the testing effect; John Sweller (UNSW) for cognitive load and worked examples; the open-spaced-repetition / FSRS maintainers (Jarrett Ye and the srs-benchmark project) for the live state of scheduling algorithms; the ALEKS / knowledge-space-theory group (Falmagne, Doignon, Cosyn) for adaptive placement; and the knowledge-tracing community (the pyKT benchmark authors) for realistic bounds on next-item prediction. Khan Academy's published tutor learnings (Kristen DiCerbo) are the best applied source on grounding AI in student state and on the adoption ceiling.

---

## DOK 2 — Knowledge tree

The graded, source-by-source notes live in the themed files: `mcat structure`, `studying methods`, `test-enhance learning`, `fsrs`, `medical anki`, and `ai generation`, plus the sections on AI tutoring, MCAT score prediction, knowledge graphs, instructor methodology, student/community, and gamification. Two synthesis documents sit on top of them and hold the adjudicated conclusions this brainlift is built from: the re-adjudication of the original brainlift against its counter (with fresh evidence per contested claim), and the placement / performance-prediction / score-display research. When a claim here and a claim in an older themed file disagree, the synthesis documents win, because they reflect the corrected, multi-source view.
