# Synapse Brainlift (v2)

*A revision of the original MCAT-app brainlift, rebuilt on stronger and better-scoped evidence. The exhaustive source summaries live in the research files (studying_methods, test-enhance_learning, fsrs, medical_anki, ai_generation) plus two consolidated research reports: the re-adjudication of v1, and the placement/performance/display study. This document keeps the thinking and the tensions; it points to those files for the raw evidence.*

# Owners
- e

# Purpose

Find every avenue for modifying and optimizing an Anki-based app for MCAT studying, grounded in learning and cognitive science, real studies, and the practices of learning-app builders, instructors, and students. The goal is an app that maximizes MCAT score per hour studied.

### In scope
- Pedagogical techniques and principles we can emulate in a desktop or mobile app.
- Going beyond flashcards into a whole MCAT studying ecosystem.
- AI features that improve student learning.
- Mechanisms of, and improvements to, spaced repetition.
- MCAT score prediction and student-performance modeling.
- **New in v2:** placement/diagnostic assessment to credit prior knowledge; per-concept mastery analytics surfaced as three scores (Memory, Performance, Readiness); honest uncertainty and abstention in anything shown to a student.

### Out of scope
- Content not tied to the MCAT.
- Live instructional classes.
- K–12 / early-childhood-specific findings, unless they generalize.

---

# How the thinking evolved (read this first)

This is a second pass. Version 1 staked out five strong opinions; we then stress-tested them against a red-team critique and a fresh trip through the primary literature, favoring 2024–2026 sources. Some opinions survived intact, a couple were partly wrong, and the exercise threw off new ideas. The tensions below aren't noise. They're where the design actually got sharper, so each SPOV and insight states the original claim, the strongest counter, and where we landed.

The headline shifts from v1:

- "Only percent-mature predicts outcomes" was too strong. Maturity is the most robust single correlate, not the sole one, and all the evidence is correlational and non-MCAT.
- "Lower retention early to add difficulty" was a category error. The deadline lever is raising retrievability *late*, not manufacturing forgetting early.
- "Expire mastered recall cards" misapplied the expertise-reversal effect. The fix is to *add* an application layer, not delete the recall card.
- The AI-grounding conclusion held, but the alarming-vs-reassuring hallucination numbers were apples-to-oranges and are now scoped correctly.
- The anti-streak stance is directionally right, but adoption is existential, so it ships as an experiment rather than a conviction.
- Genuinely new this round: a placement assessment that credits prior knowledge but never on a single answer, and a three-number analytics model that has the discipline to show nothing when the data can't support a number.

---

# DOK 4 — Spiky Points of View

Six opinions, deliberately spread across different axes so they don't collapse into one idea: **what cards exist** (1), **how they're scheduled** (2), **how they evolve** (3), **how AI creates and repairs them** (4), **how the student is motivated** (5), and **how learning is measured and communicated** (6). The two I'd defend as most novel-yet-grounded are SPOV 2 (a deadline governor that peaks late) and SPOV 6 (three honest numbers, with the courage to show none).

### SPOV 1 — Mint cards from mistakes, but treat coverage as a requirement, not an afterthought.

**Claim.** The premade mega-deck ground "to completion" is MCAT prep's worst artifact. Completion is a vanity metric, and the best cards are minted on demand from your own missed questions and tied to the item that exposed the gap.

**Tension.** The vanity-metric critique is fair: maturity, not volume, is the correlate that tracks outcomes, and bare cards sit in the low-transfer regime a reasoning exam punishes. But premade decks are a proven, zero-setup, high-coverage tool, and there's a verified existence proof of a student going 508→520 on essentially a premade deck plus practice. Pure error-minting has a real coverage-gap risk (you never make a card for high-yield material you happen not to have been tested on, and psych/soc is broad enough that this bites), and "percent mature" is itself gameable and only a proxy.

**Where we land.** Error-driven minting is the *default* path, but additive, not exclusive. Ship an opt-in vetted coverage deck, a coverage checker against the AAMC content outline, and a placement assessment to seed prior knowledge (SPOV 6 / Insight 11). The home metric is mastered, test-ready concepts, never "cards remaining."

**Supporting research (multiple).** Wright State and UNLV cohorts (maturity most robust, but hours and unsuspended-card count also correlate; all correlational, single-site, non-MCAT); Pan & Rickard (bare-recall transfer d≈0.28); Jack Westin ("Practice finds weaknesses; Anki prevents re-forgetting"); SDN existence proof; ALEKS/CAT placement efficiency.

**Honest note.** "Questions first, cards downstream" is repackaged best practice, not an invention. The novelty is frictionless minting, automatic linking, and coverage safety, not the idea itself.

### SPOV 2 — Schedule toward test day by raising retrievability late, not by forgetting early.

**Claim.** A dated exam wants a schedule that peaks recall on test day. Almost no tool, Anki included, schedules against a deadline at all, because FSRS optimizes for indefinite retention.

**Tension.** Version 1's move was to "tolerate more forgetting early" by lowering desired retention. That's a category error on two counts. First, FSRS workload is U-shaped in desired retention: set it too low and you do *more* work, not less, because cards lapse and you pay the relearning tax. The FSRS docs are explicit that setting retention below the recommended value is "actively detrimental for studying efficiency." Second, Bjork's desirable difficulty is the *spacing gap*, which FSRS already operationalizes by scheduling a review when predicted recall hits the target. Lowering the retention *parameter* is a different lever, and a difficulty is only desirable if the retrieval still succeeds. Cepeda's ridgeline, meanwhile, is a two-event result, not a validated multi-review deadline curve.

**Where we land.** Keep FSRS at its workload-efficient retention (~0.85–0.90). Make it deadline-aware by raising effective retrievability and compressing intervals in the final two-to-three weeks, so the curve peaks late rather than dipping early. Ship the test-date governor as an A/B against flat FSRS, with a kill criterion: cut it if total review load rises or practice scores fall.

**Supporting research.** FSRS workload / optimal-retention documentation (simulation on large community data); Bjork & Bjork (New Theory of Disuse; a difficulty is desirable only if prerequisites are met); Cepeda et al. 2008 (optimal gap shrinks as the deadline nears, but only across two study events); Expertium (true average retrievability runs above the desired-retention setting).

**Honest note.** The *problem* (FSRS is deadline-agnostic) is real and unaddressed by existing tools. The *specific* peak-late curve is unproven, which is exactly why it's an experiment, not a default.

### SPOV 3 — A recall card graduates by gaining an application layer, not by dying.

**Claim.** The MCAT almost never asks "what is X"; it asks you to wield X in a novel context. So pure-recall cards should evolve into passage-grounded application items, and mastery should be judged on the application form.

**Tension.** Version 1 said mastered recall cards should *expire*, citing the expertise-reversal effect. That misreads it. Expertise reversal is about *instructional guidance* (worked examples, redundant explanations) becoming unnecessary for experts. Retrieval practice isn't guidance; it's the desirable difficulty itself, and nothing in expertise reversal says stop retrieving mastered facts. Worse, the New Theory of Disuse says retrieval strength decays without use, so a retired "mastered" fact quietly rots before a fixed-date exam. And "the MCAT never asks what X is" is overstated: psych/soc rewards precise discrimination between near-identical terms, and amino acids, hormones, and equations are legitimately recall.

**Where we land.** Metamorphose by *adding* application items on top of recall, and let FSRS stretch the mastered-recall interval (which already makes it cheap to maintain). Retire the bare-recall version only after mastery on the *application* form, and keep light maintenance retrieval before the exam. Gate metamorphosis on genuine stability, and skip the irreducibly factual.

**Supporting research.** Pan & Rickard (response congruency lifts transfer from d≈0.28 to 0.58; elaborated retrieval adds ~0.23); Karpicke & Blunt (retrieval reaches inference — direction solid, though the d≈1.50 magnitude is contested by Mayrhofer et al. 2023 as partly a memorization-time artifact); Kalyuga & Sweller (the actual scope of expertise reversal); Bjork & Bjork (disuse and decay); Rohrer (interleaving forces strategy selection, 61% vs 38%).

### SPOV 4 — AI is a grounded transformer with a QA gate, and its best use is re-teaching, not mass-production.

**Claim.** AI must be forbidden from inventing facts and restricted to transforming verified ones. Its flagship use is repairing failing cards, not spitting out new ones.

**Tension.** The hallucination evidence looked like a clean "ungrounded is catastrophic, grounded is safe" story, but the two headline numbers are incommensurable. The alarming ~50–82% is an *adversarial* worst-case where a false detail was deliberately planted in the prompt (Omar et al. 2025). The reassuring ~1.47% is a *summarization* task using *direct GPT-4 prompting, not RAG* (Asgari et al. 2025). So the quantitative case was built from non-comparable measures, even though the *conclusion* (ground the AI) is right. Grounding reduces hallucination sharply but never to zero (best clinical RAG configs sit around 5–6%). Strict grounding also carries real costs: a vetted-source pipeline, slower generation, less coverage, and it isn't a unique moat (Blueprint and Jack Westin already do some grounded generation).

**Where we land.** Sandbox AI to source-grounded transformation, with a citation to the grounding source on every item, a rule-based item-flaw checker (more reliable than an LLM judge, roughly 91% vs 79% flaw detection), and human review before anything reaches a learner. The flagship use is early leech repair: at the first one or two lapses, atomize the card, detect the interfering sibling, run a ~60-second worked-example micro-lesson, then reschedule. Budget for the pipeline cost and set a residual-hallucination monitoring target.

**Supporting research.** Omar et al. 2025 (adversarial ≤82%); Asgari et al. 2025 (summarization 1.47%, direct prompt, not RAG); Chelli et al. 2024 (reference fabrication 28.6–91.4%); Doughty et al. 2024 (MCQ defect rates ~4–5% vs ~1% human); rule-based-vs-LLM item-quality judging; Anki "Leeches" + Wozniak (interference as the chief cause of forgetting); Khanmigo (grounding in student state, +6.1% unaided next-item correctness).

### SPOV 5 — Measure learning, not activity, and reward the hard thing, without killing adherence.

**Claim.** Reward durable, difficult behaviors (nailing a card after a long lapse-free gap, resurrecting a lapsed card, answering delayed application items unaided) and strip points for easy-rep padding and streak-protection.

**Tension.** The learning-purity concern is real: speed-running an easy lesson to protect a streak "activates the retention mechanism without the learning," Shortt et al. found Duolingo's gains concentrated in low-order vocabulary, and restudy actively inflates confidence while worsening retention. But streaks and loss-aversion mechanics are among the most robust adherence drivers known (Duolingo reports a streak feature lifting Day-7 retention ~14%, and a 7-day streak correlating with several-times-higher daily engagement, both vendor figures), stripping rewards risks losing the disengaged, procrastination-prone students who most need scaffolding (overjustification effect), and even an excellent grounded tutor hits an adoption ceiling (Khanmigo, ~15% engagement among eligible students). An app nobody opens teaches nothing, which is the single biggest product risk in the whole proposal.

**Where we land.** Put learning-aligned metrics up front (the three scores of SPOV 6; de-emphasize counts and streaks), and weight rewards toward difficult successful retrievals and recoveries. But keep low-pressure adherence mechanics (streak-freeze / forgiveness so one missed day during crunch doesn't wipe progress), and ship the anti-streak design as an A/B guarded on daily/weekly active use and 4-week retention.

**Supporting research.** Bjork & Bjork (performance ≠ learning; illusion of competence); Roediger & Karpicke (restudy inflates confidence while worsening retention); Duolingo engineering (vendor retention stats); Shortt et al. 2023; Tversky & Kahneman (loss aversion, λ≈2.25); Khanmigo adoption ceiling; Wright State (maturity, not volume).

### SPOV 6 (NEW) — Three honest numbers, or none at all.

**Claim.** Show Memory (can you recall it now), Performance (can you apply it to a novel exam-style item), and Readiness (what you'd score today) as *distinct* numbers at every grain — per concept, per section, whole exam — each with a range, its drivers, and its coverage. And refuse to show any score the data can't support.

**Why it's spiky.** Every other app collapses progress into one confident figure: a percent-mastery bar, a streak, a single predicted score. But recall is not transfer is not readiness, and a lone confident number is precisely the illusion of competence the app exists to fight. The genuinely hard part is Performance for concepts a student has never been tested on in application form: knowledge tracing tops out around 0.70–0.82 AUC and is near chance at cold start, and MCAT prediction is unreliable above ~515. So the honest move, which is rare in edtech, is to quantify uncertainty openly and sometimes decline to answer.

**Where we land.** Memory is essentially FSRS retrievability: cheap, well-validated, narrow range. Performance is Memory discounted by a transfer factor (a bigger discount when recall is weak, the format differs, or the task is application), calibrated against unaided next-item correctness, with wide ranges and prerequisite-graph priors for untested concepts. Readiness is an AAMC-anchored blend with documented third-party offsets, widening its range and abstaining at the extremes. Every score carries a point estimate, a plain-language range, coverage %, a "how sure" indicator, a last-updated timestamp, and its top drivers, plus a give-up rule that withholds the number when coverage is too low, the interval too wide, or calibration too poor.

**Supporting research (multiple, convergent).** Soderstrom & Bjork and Koriat & Bjork (performance ≠ learning; illusion of competence); Pan & Rickard (the recall→transfer discount); the pyKT benchmark and Gervet et al. (knowledge-tracing AUC ceiling ~0.70–0.82); Zhang et al. 2021 (cold-start prediction near chance); Chen & Corridon 2020 plus the MCAT's ±2 SEM (prediction, and its unreliability at the top); ALEKS / knowledge-space theory and IRT-CAT (placement, and never crediting mastery on one answer); selective prediction / the reject option (Chow; El-Yaniv & Wiener; Geifman & El-Yaniv); uncertainty communication (Irwin & Mandel); learning-analytics dashboard reviews (dashboards often don't help, and rarely explain their numbers).

---

# DOK 3 — Insights

## Theme A — The retention–application gap is the whole game

**Insight 1.** Anki's effectiveness ceiling for the MCAT is set by *transfer*, and transfer is a property of card *form*, not of flashcards themselves. Retrieval practice does transfer to novel and inference items, but only moderately (d≈0.40), and conditionally: about 0.28 without response congruency, near zero when initial recall is weak, and roughly +0.23 when retrieval is elaborated. Bare cued-recall and cloze cards sit squarely in that low-transfer regime, so the lever is moving cards into elaborated, application-framed retrieval rather than arguing "flashcards vs practice tests." *Sources:* Pan & Rickard 2018; Karpicke & Blunt 2011; Dunlosky et al. 2013; MCAT/CARS structure. *Tension:* Mayrhofer et al. 2023 shows part of the classic retrieval-vs-mapping margin was a memorization-time artifact — the direction is safe, the exact size less so.

**Insight 2.** Retrieval can reach higher-order learning, so the cure for reasoning is *better* retrieval, not less of it. Karpicke & Blunt found retrieval beat elaborative concept mapping even on inference items, and Pan & Rickard's elaborated-retrieval bonus says we can push retrieval up Bloom's levels. One distinction to keep straight: *elaborated retrieval* (enriching the retrieval act with explanation prompts or higher-order questions) helps, whereas *elaborative study* (concept mapping used instead of retrieving) is what loses to retrieval. They don't conflict. *Sources:* Karpicke & Blunt 2011; Pan & Rickard 2018.

## Theme B — Measure the right thing

**Insight 3.** Maturity is the most robust Anki correlate of outcomes, but not the sole predictor, and the whole evidence base is thin. In the Wright State cohort, percent-mature was the only independent predictor for one of four outcomes; the UNLV cohort found mature cards, study hours, and unsuspended-card count all correlated (they're collinear); Wothe found daily use predicted Step 1 but not Step 2. The often-garbled "1,500 cards per point" traces to Deng et al. 2015, which actually reports about one Step-1 point per ~1,700 unique cards *or* ~445 practice questions, and it's correlational and pre-AnKing. *Corollary:* the north-star is mature, retrievable, *applicable* concepts, and the three-number model (SPOV 6) makes "applicable" explicit. *Tension:* v1 overstated "only maturity"; every study here is correlational, single-site, and non-MCAT.

## Theme C — Scheduling is a deadline problem (but mind the lever)

**Insight 4.** The highest-value scheduling move for a dated exam is deadline-awareness, but the lever is raising retrievability late, not lowering it early. FSRS beats SM-2 decisively and cuts reviews ~20–30% for equal retention (simulation), and it exposes a desired-retention knob, but its workload is U-shaped in that knob, so lowering it too far costs more work through relearning. Cepeda supplies the shape (optimal gap shrinks as the deadline nears), FSRS supplies the mechanism, and Bjork clarifies that the desirable difficulty is the spacing gap FSRS already implements. *Tension:* v1's "lower retention early" conflated the retention parameter with the spacing gap and is a category error.

## Theme D — Failure modes are diagnostic signal

**Insight 5.** Ease hell, leeches, and burnout are the system mistaking a *card-quality* problem for a *scheduling* problem, which is the single best place to insert AI. SM-2 shrinks intervals on repeated failure; Anki flags a leech only after 8 lapses and its own manual blames the card, usually for interference between similar items; Wozniak calls interference "the single greatest cause of forgetting." So a chronically failing card is diagnostic, and the right response is to reformulate or re-teach early, not to reschedule harder. *Sources:* SM-2 mechanics; Anki "Leeches"; Wozniak's 20 Rules / minimum information principle.

## Theme E — AI: grounding is the bright line

**Insight 6.** The wildly conflicting hallucination numbers resolve into one rule once you sort them by task: AI may *transform* vetted content but must never *originate* facts. Ungrounded/adversarial medical generation fabricates in up to ~82% of cases and invents references freely; a source-constrained summarization task runs ~1.47% (though that figure is direct prompting, not RAG); grounded RAG lands around 5–6% residual, not zero. Generated MCQs match human quality on average but carry higher specific-defect rates. *Tension:* the numbers were incommensurable, the conclusion is sound, and grounding reduces without eliminating. *Sources:* Omar 2025; Asgari 2025; Chelli 2024; Doughty 2024; RAG studies.

**Insight 7.** AI tutoring's measurable payoff comes from grounding it in the student's *state*, not from the model's eloquence, and the gains accrue to practice. Khanmigo improved unaided next-item correctness by +6.1% overall, with +2.7% specifically from surfacing unmastered prerequisites and +3.4% from problem-solving history. *Corollary:* wire the tutor into per-concept mastery and the knowledge graph, and make it proactive, because even this tutor sees only ~15% engagement. *Sources:* Khan Academy learnings 2025–26.

## Theme F — Performance ≠ learning (the unifying error)

**Insight 8.** Streaks, review counts, fluency, and self-rated confidence are the same trap in four costumes: performance/engagement proxies that can move opposite to learning. Bjork's storage-vs-retrieval framing holds that observable performance is an unreliable index of durable learning; restudy inflates confidence while worsening retention; streak-protection activates the retention mechanism without the learning. *Corollary:* reward and display delayed, unaided retrieval and transfer, which is exactly what the Performance score (SPOV 6) is. *Sources:* Bjork & Bjork; Roediger & Karpicke; Duolingo critiques; Khan Academy.

## Theme G — One deck can't fit everyone

**Insight 9.** Expertise reversal makes a static premade deck structurally wrong: the same hard card is a desirable difficulty for one student and damage for another, so difficulty must be titrated to per-concept mastery. Novices need worked examples and guidance; that advantage recedes and can reverse as expertise grows; and a difficulty is only desirable if the learner has the prerequisites, with transfer near zero when initial retrieval fails. This makes the prerequisite knowledge graph a correctness requirement, not a nice-to-have. *Tension:* expertise reversal is about *guidance*, not about retiring retrieval practice (see SPOV 3). *Sources:* Kirschner/Sweller/Clark 2006; Bjork & Bjork; Pan & Rickard 2018; Math Academy FIRe.

## Theme H — Direction of flow: questions first

**Insight 10.** Flashcards belong *downstream* of missed practice questions, not upstream of them: practice is the diagnostic engine, cards are the retention patch. Interleaved practice forces the strategy-selection the exam tests, retrieval transfers best when it mirrors the criterion task, and AAMC full-lengths are the gold-standard readiness signal. *Tension:* this is established best practice (Jack Westin, Blueprint, MedLife, high-scorer consensus), not a novel invention; the contribution is automating and linking it. *Sources:* Jack Westin; Rohrer et al. 2020; Pan & Rickard 2018; AAMC FLs.

## Theme I (NEW) — Credit what they already know, but only when it's confirmed

**Insight 11.** A placement/diagnostic assessment can cut redundant study for students who arrive with prior prep, but a single correct answer is a false-positive mastery signal. Knowledge-space systems locate a knowledge state efficiently (ALEKS placement AUROC ≈ 0.89, with "known" topics answered correctly ~83% vs ~8% for "unknown"), and IRT-based adaptive testing reaches equal precision in roughly half the items of a fixed test. But performance in the moment overstates durable learning and fluent-feeling answers are unreliable, so mastery credit needs an application-level item, at least one spaced confirmation, and tolerance for slips; anything short of that seeds a shortened interval rather than full mastery. *Tension:* much of the ALEKS efficacy evidence is vendor-authored and rigorous proof in high-stakes exam prep is thin, so our own rollout is the experiment (A/B placement-credited vs full-review, measured on delayed retention and final scores). *Sources:* ALEKS knowledge-space evaluation; IRT/CAT efficiency; Soderstrom & Bjork; Koriat & Bjork.

## Theme J (NEW) — Three numbers, honestly ranged, and the courage to show none

**Insight 12.** Memory, Performance, and Readiness are distinct quantities that routinely diverge, and the honest system shows all three with ranges, drivers, and coverage, and abstains when the data is thin. Memory is close to FSRS retrievability, so it's cheap and trustworthy. Performance is the hard one, because recall-to-transfer is a discount (Pan & Rickard), knowledge tracing tops out around 0.70–0.82 AUC, and cold-start accuracy on never-attempted concepts is near chance unless prerequisite signal helps. Readiness is thin-evidenced (one small MCAT-prediction study, a built-in ±2 SEM, unreliable above ~515), so it widens and abstains at the extremes. The abstention itself is principled, borrowed from selective-prediction / reject-option work, and ranges plus explained drivers are what the uncertainty-communication and dashboard literatures say non-experts actually read correctly. This insight is the measurement backbone that makes SPOV 6 concrete. *Sources:* Soderstrom & Bjork; Koriat & Bjork; Pan & Rickard 2018; pyKT benchmark and Gervet et al.; Zhang et al. 2021 (cold-start); Chen & Corridon 2020; selective prediction (Chow; El-Yaniv & Wiener; Geifman & El-Yaniv); uncertainty communication (Irwin & Mandel); learning-analytics dashboard reviews.

---

# Experts to Follow

**Piotr Woźniak (SuperMemo).** Invented computational spaced repetition (SM-0 → SM-18), co-authored the two-component model of memory (stability + retrievability) that FSRS's DSR model descends from, and wrote the minimum information principle and the "20 Rules of Formulating Knowledge." Names interference the single greatest cause of forgetting in a mature collection. The whole scheduling lineage traces to him, and his card-formulation rules are the most actionable spec we have for card-quality enforcement (atomicity, interference detection). Caveat: he now publishes raw ideas on his wiki rather than peer-reviewed work, so treat it as brilliant primary-practitioner material, not vetted literature. supermemo.guru; @SuperMemoWoz.

**Jeffrey Karpicke (Purdue).** The most prolific contemporary investigator of retrieval-based learning: retrieval changes memory and beats elaborative study, including on inference items, and students hold strong metacognitive illusions about it. The evidentiary backbone for two commitments: content should recur as retrieval, and we shouldn't trust learner-reported confidence. learninglab.psych.purdue.edu.

**Robert & Elizabeth Bjork (UCLA).** The learning-vs-performance distinction, the New Theory of Disuse (storage vs retrieval strength; biggest storage gains from effortful successful retrieval), desirable difficulties, and the illusion of competence. They supply the theory that makes the contrarian SPOVs defensible, and the crucial boundary condition that a difficulty is only desirable if the learner can meet it. bjorklab.psych.ucla.edu.

**Doug Rohrer (USF).** Interleaved practice improves STEM learning by forcing strategy selection, shown in classroom RCTs (61% vs 38%, d≈0.83), and spacing reduces overconfidence. The key source for why the app must mix problem and passage *types*, with preregistered real-classroom evidence. uweb.cas.usf.edu/~drohrer.

**Justin Skycak (Math Academy).** Treats a STEM curriculum as a prerequisite knowledge graph and documents the underlying algorithms (mastery learning, spaced repetition, interleaving, and Fractional Implicit Repetition, which gives discounted trickle-down credit to prerequisites). The closest existing product to what we're building. Caveat: a practitioner/company perspective, and figures like "4x speed" are marketing, not independent research. justinmath.com.

**Adjacent, lighter follows.** John Dunlosky (Kent State) for periodic "what actually works" reviews; Henry Roediger III (WashU) for the testing effect; John Sweller (UNSW) for cognitive load and worked examples; the open-spaced-repetition / FSRS maintainers (Jarrett Ye) for the live state of scheduling and benchmarks. **New in v2, for the placement and scoring work:** knowledge-space theory (Falmagne & Doignon; the ALEKS research line, e.g. Cosyn et al.) and computerized adaptive testing / IRT for placement; the knowledge-tracing benchmark community (pyKT; Gervet et al.) for honest limits on predicting unseen-item correctness; and risk/uncertainty communication (e.g. Mandel) for how to show ranges without misleading.

---

# DOK 2 — Knowledge map

The detailed source summaries live in the linked research files and the two consolidated research reports; this is the index, not a reproduction.

- **A. Learning science — what works.** Practice testing and distributed practice are highest-utility; interleaving, self-explanation, elaborative interrogation are moderate; rereading and highlighting are low. → `studying_methods.md`
- **B. Limits of testing-effect transfer.** Transfer is modest (d≈0.40) and conditional on response congruency, elaboration, and initial success. → `test-enhance_learning.md`
- **C. Spaced repetition — FSRS & SuperMemo.** DSR model; FSRS beats SM-2 on log loss; workload is U-shaped in desired retention; the minimum information principle and 20 Rules. → `fsrs.md`
- **D. Medical & MCAT Anki evidence.** Maturity is the most robust correlate but not the sole predictor; all correlational, single-site, non-MCAT; Deng's ~1,700-cards-or-445-questions-per-point figure. → `medical_anki.md`
- **E. AI generation & hallucination.** Ungrounded/adversarial fabrication up to ~82%; grounded summarization ~1.47% (direct prompt, not RAG); RAG residual ~5–6%; MCQ defect rates ~4–5% vs ~1% human. → `ai_generation.md`
- **F. AI tutoring.** Khanmigo: +6.1% unaided next-item correctness from grounding in student state; ~15% adoption ceiling.
- **G. MCAT score prediction.** AAMC full-lengths are the gold standard; third-party exams deflate (Kaplan ≈ +10 best-corroborated); prediction weakest above ~515; one small peer-reviewed study (Chen & Corridon, n=19).
- **H. Knowledge graphs, mastery & implicit review credit.** Math Academy FIRe (trickle-down credit, repetition compression, per-topic spacing); Bloom's mastery learning / 2-sigma (direction, not magnitude).
- **I. Instructor MCAT methodology.** Jack Westin ("do-not-forget-it, not learn-it"; tie cards to passage mistakes); MedLife; the practice-driven prep consensus.
- **J. Student / community.** The deck wars (AnKing, MileDown, JackSparrow); high-scorer accounts using premade decks + heavy practice; the lived failure modes (pile-ups, ease hell, burnout).
- **K. Gamification & motivation.** Duolingo streak/loss-aversion retention gains (vendor); Shortt et al. (gains mostly vocabulary); prospect theory (λ≈2.25); overjustification risk.
- **L (NEW). Placement & adaptive assessment.** Knowledge-space theory / ALEKS (AUROC ≈ 0.89, vendor); IRT-based CAT (~50% fewer items for equal precision); standard-error and predicted-standard-error-reduction stopping rules.
- **M (NEW). Learner modeling & performance prediction.** Knowledge tracing (BKT, DKT, AKT); the pyKT benchmark and Gervet et al. (AUC ceiling ~0.70–0.82); cold-start accuracy near chance on unseen concepts.
- **N (NEW). Uncertainty communication & selective prediction.** The reject option / selective prediction (Chow; El-Yaniv & Wiener; Geifman & El-Yaniv); inconsistent reading of verbal probabilities (Irwin & Mandel); learning-analytics dashboards often fail to help and rarely explain their numbers.
