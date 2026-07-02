
# Owners

- e

# Purpose

Researching all possible avenues of how to modify and optimize the Anki app for MCAT studying, grounded in learning/cognitive science, research studies, learning app developers, students, etc.

### In Scope

- Any sort of pedagogical techniques and principles we can emulate in a desktop or mobile app.
- Going beyond simple flashcards, crafting whole MCAT studying ecosystem.
- AI features to improve student learning.
- Mechanisms of and improvements to spaced repetition.
- MCAT score prediction; modeling student performance.

### Out of Scope

- Content that isn't directly tied to the MCAT.
- Live, instructional classes.
- K-12 / early-childhood-specific findings, unless they generalize.

---

# DOK 4 Spiky Points of View

### SPOV 1: Ship no deck to finish. The premade mega-deck is MCAT prep's worst artifact — "completion" is a vanity metric, and cards should be minted from your own mistakes.

MCAT Anki culture (AnKing, MileDown, JackSparrow) treats grinding a 5,000-card premade deck to completion as _the_ goal. But the only Anki statistic that predicts outcomes is percent _mature_ — card count, review count, retention rate, and ease predict nothing — and bare premade cards sit in the low-transfer regime a reasoning exam punishes. Completing someone else's deck runs the causal arrow backwards: memorize-then-pray-it-transfers.

- **Solution:** The app ships with _zero_ "complete-me" deck. Every card is minted on demand from a missed practice question or a flagged content gap, auto-elaborated into application form, and permanently linked to the item that spawned it. Cold start is handled by a short worked-example diagnostic that seeds the knowledge graph — not by dumping 5,000 cards. The home screen shows "concepts mature & test-ready" and passage accuracy, never "cards remaining."
- **Supporting Research:** Wright State cohort (only %mature correlated with performance; volume did not); Jack Westin ("do-not-forget-it, not learn-it"; tie cards to passage mistakes); Pan & Rickard (bare-recall transfer d≈0.28).
- **Forged from:** Insights 3 + 10 + 1.
- **Steelman counter:** Premade decks lower activation energy and guarantee coverage; pure error-driven minting risks blind spots for topics never practiced — so the diagnostic seeding and a coverage-checker against the AAMC content outline are load-bearing, not optional.

### SPOV 2: Stop maxing retention. For a dated exam the right target is a _rising curve_ that deliberately tolerates more forgetting early — and "fewer reviews" is the wrong thing to optimize.

The FSRS world defaults to 90% desired retention and markets itself on review reduction. But two findings cut against flat-90%-forever: Bjork's desirable difficulties say each _successful_ retrieval from a _lower_ retrieval-strength state builds more durable memory, and Cepeda shows optimal spacing compresses as the deadline nears. A flat target is therefore doubly wrong — too high early (squandering effort on over-learned cards and forfeiting the potency of harder retrievals) and not high enough late (under-preparing for the one day that counts).

- **Solution:** A "test-date governor" sets desired-retention as a curve, not a constant — a deliberately leaner target far out (more spacing, harder-but-still-mostly-successful retrievals) ramping to near-ceiling in the final ~2–3 weeks, with intervals compressing toward exam day. The objective is _retrievability on your test date_, not steady-state retention or minimal reviews. A hard floor stops the target dropping so low that cards lapse (a failed retrieval is a wasted rep).
- **Supporting Research:** Cepeda et al. (optimal gap shrinks from ~20–40% to ~5–10% of the retention interval); Bjork & Bjork (storage gain greatest at low retrieval strength); FSRS docs (the knob exists; the default is deadline-unaware).
- **Forged from:** Insights 4 + 8.
- **Steelman counter:** "Lower retention early is better" extrapolates desirable-difficulty _theory_ down to an FSRS _parameter_ not directly trialed; pushed too far it just causes relearning. The defensible claim is a _calibrated rising curve_, A/B-validated against flat 90% before it's a headline feature.

### SPOV 3: A flashcard is a chrysalis, not a permanent home. Pure-recall cards should _expire_ at mastery and metamorphose into passage-grounded application items — and you should be graded on the application form.

The MCAT almost never asks "what is X"; it asks you to wield X inside a novel experiment, and CARS asks for nothing but reasoning. Bare recall trains the low-transfer regime, and expertise-reversal research says that once a fact is mastered, drilling recall further is redundant cognitive load. Yet every flashcard tool treats a matured card as a trophy to review forever.

- **Solution:** "Card metamorphosis." A fact enters as a simple recall card; once retrieval is stable, the app _stops scheduling the bare-recall version_ and promotes the concept into an interleaved, passage-embedded application item that forces the fact's use in a new context. Mastery is measured on the application form — so "knowing it" means "can apply it under passage conditions," not "can parrot the definition."
- **Supporting Research:** Pan & Rickard (response congruency; elaborated retrieval +d≈0.23); Karpicke & Blunt (retrieval reaches inference, d≈1.50); expertise-reversal / worked-example effect (Kirschner, Sweller & Clark); Rohrer (interleaving forces strategy selection, 61% vs 38%).
- **Forged from:** Insights 1 + 2 + 9.
- **Steelman counter:** Some MCAT content genuinely _is_ recall (amino-acid structures, hormones, equations), where a durable bare-recall card is exactly right; premature promotion could destabilize a not-yet-solid fact. Metamorphosis must gate on real stability and skip the irreducibly factual.

### SPOV 4: Every "AI study app" is built backwards. AI must be forbidden from inventing facts and restricted to transforming verified ones — and the best use of that AI is to _re-teach_ failing cards, not mass-produce new ones.

The current wave of AI flashcard/quiz generators spins content out of a model's parametric memory — exactly the ungrounded regime where medical hallucination runs up to ~83% and references are fabricated wholesale, while _source-grounded_ transformation runs ~1.47% (≈ human). Meanwhile Anki diagnoses a "leech" only after 8 painful lapses and blames the user, when its own manual says the _card_ is the problem. Both are the same error: trusting generation over grounding.

- **Solution:** AI is sandboxed to _source-grounded transformation_ only — summarize _this_ AAMC-aligned passage into atomic cards, write distractors from _this_ verified answer key, explain _this_ correct solution — with a verification gate and a citation to the grounding source on every item; anything it can't ground, it won't emit. The flagship use of that AI is **leech repair**: at the _first_ couple of lapses, it atomizes the card, detects the interfering sibling, and inserts a 60-second worked-example micro-lesson before rescheduling. Leeches trigger teaching, not suspension.
- **Supporting Research:** Asgari et al. (grounded 1.47%); RAG studies (0% vs 8%); Omar et al. (ungrounded ≤83%); Chelli et al. (fabricated references); Doughty et al. (GPT-4 MCQ defect ~4–5% vs ~1% human); Anki "Leeches" + Wozniak (interference as chief cause of forgetting).
- **Forged from:** Insights 5 + 6 + 7.
- **Steelman counter:** Strict grounding caps what the AI can do and adds content-pipeline cost (you must supply vetted sources); a looser generator ships faster and covers more. The bet — that one confidently-wrong card costs more than a smaller, slower, grounded library — holds _only if_ the grounding/QA gate is actually enforced.

### SPOV 5: Gamify forgetting, not streaks. Reward the hard, durable behaviors every other app punishes; strip points for the easy reps every other app celebrates.

Consumer edtech reflexively copies Duolingo's streak/XP/league loop, but those are engagement proxies that diverge from learning: speed-running easy lessons to protect a streak "activates the retention mechanism without the learning," and Bjork's illusion of competence shows fluency _feels_ like mastery while predicting little. Restudy even _inflates_ confidence as it _worsens_ retention. Standard gamification optimizes precisely the wrong variable — and a reasoning exam will expose it.

- **Solution:** An "anti-streak" reward system. Points accrue for _desirable difficulty_: nailing a card after a long lapse-free gap (more points the longer the gap), resurrecting a lapsed/leech card, and answering _delayed application items unaided_. Points are withheld or removed for binge-cramming young cards and review-count padding. The home metric is "concepts mature & test-ready" plus a test-day readiness projection — no flame, no streak, no XP-for-volume.
- **Supporting Research:** Duolingo critiques (streak-protection ≠ learning; engagement/learning divergence); Bjork & Bjork (performance ≠ learning; illusion of competence); Roediger & Karpicke (restudy inflates confidence, worsens retention); Khan Academy (optimize _unaided delayed_ correctness); Wright State (maturity, not volume).
- **Forged from:** Insights 8 + 3 + 1.
- **Steelman counter:** Streaks demonstrably drive adherence, and an app no one opens teaches nothing; punishing engagement risks losing the disengaged students who most need it. The fix is to make the _rewarded_ behaviors still feel good and habitual (daily readiness gains, recovery wins), not to strip motivation design wholesale.

Notice they're deliberately spread across five different axes so they don't collapse into one idea — **what cards exist** (1), **how they're scheduled** (2), **how they evolve** (3), **how AI creates/repairs them** (4), and **how the student is motivated** (5). Together they describe an app that is recognizably _not_ "Anki with a chatbot bolted on." The two I'd defend as the most genuinely novel-yet-grounded are **SPOV 2** (a deadline-rising retention curve that embraces early forgetting — almost nobody does this, and it falls straight out of Cepeda + Bjork) and **SPOV 3** (cards that expire into application items — it redefines the unit of study around what the MCAT actually tests).

---

# DOK 3 Insights

## Theme A — The retention–application gap is the whole game

### Insight 1: Anki's effectiveness ceiling for the MCAT is set by _transfer_, and transfer is a property of card form, not of flashcards themselves.

The MCAT rewards applying knowledge to novel passages and data, and CARS is pure reasoning with zero memorizable content. The test-enhanced-learning literature confirms retrieval practice _does_ transfer to novel and inference items — but only moderately (d ≈ 0.40), and conditionally: it falls to d ≈ 0.28 when the practiced response doesn't overlap the tested one, approaches zero when initial recall is weak, and gains ~0.23 when retrieval is "elaborated." Bare cued-recall and cloze cards — the Anki default — sit squarely in that low-transfer regime. The epiphany is that the real lever isn't "flashcards vs. practice tests"; it's moving cards _out_ of bare recall into elaborated, application-framed retrieval (explain-why prompts, "which principle applies," data-snippet stems). That preserves spaced repetition's retention engine while buying the transfer the exam actually grades.

- **Source Connection:** Pan & Rickard (2018, d=0.40 / 0.28 / +0.23); Karpicke & Blunt (2011); Dunlosky et al. (2013); MCAT/CARS structure.
- **SPOV Connection:** → _Prospective SPOV: "A card that only tests recall is a half-built card for a reasoning exam."_

### Insight 2: Retrieval practice can reach higher-order learning — so "Anki is only good for low-yield facts" understates it, and the cure for reasoning is _better_ retrieval, not less of it.

This is the optimistic mirror of Insight 1. Karpicke & Blunt found retrieval beat elaborative concept-mapping by a full d ≈ 1.50 on a delayed test — including on inference questions, and even when the final test was _building a concept map_ — while students wrongly predicted the reverse. Combined with Pan & Rickard's "elaborated retrieval" bonus, this says the problem with flashcards-for-reasoning isn't an inherent ceiling on what retrieval can do; it's that most cards never _ask_ for reasoning. An app can therefore push retrieval up Bloom's levels rather than capping it at recognition/recall.

- **Source Connection:** Karpicke & Blunt (2011); Pan & Rickard (2018). _Caveat: a 2024 critique argues part of Karpicke & Blunt's margin is a memorization-time artifact — the direction is safe, the exact size less so._
- **SPOV Connection:** → _Prospective SPOV: "Fuse the retention engine with an application engine."_

## Theme B — Measure the right thing

### Insight 3: The only Anki statistic that tracks outcomes is _maturity_, not volume — quiet evidence against the deck-completion arms race.

In the Wright State cohort, Anki users outscored non-users (+6–7% on courses, +12.9% on the CBSE), yet of ~21 user statistics, almost none correlated with performance — not total cards, not review count, not retention rate, not ease. The lone consistent predictor was percent of cards _mature_ (≥21-day interval), explaining ~36% of variance in one course. Wothe likewise found daily use helped Step 1 (p=.039) but concluded "a variety of methods achieve similar outcomes." The connection: outcomes track _successfully retained, well-spaced concepts_ — not cards created or hours ground. So the product's North-Star metric should be "mature, retrievable concepts," and the UI should deliberately de-emphasize the card counts and streaks students optimize by reflex.

- **Source Connection:** Gilbert et al. / Wright State (2023); Wothe et al. (2023). _Caveat: both are correlational, single-cohort, with shrinking samples._
- **SPOV Connection:** → _Prospective SPOV: "Count mastery, not activity."_

## Theme C — Scheduling is a deadline problem

### Insight 4: For a fixed test date, the highest-value scheduling move isn't "more efficient reviews" — it's making spacing and retention _deadline-aware_, which inverts FSRS's default posture.

FSRS beats SM-2 decisively (lower log loss in 99.6% of collections; ~20–30% fewer reviews for equal retention) and exposes a desired-retention knob. But the deeper principle comes from Cepeda et al.: the _optimal_ gap is a shrinking fraction of the retention interval — ~20–40% at a one-week horizon, down to ~5–10% at a one-year horizon. The MCAT has a hard deadline, so the app should _compress_ gaps as test day approaches and _raise_ desired retention in the final weeks — the opposite of "set 90% and forget." FSRS supplies the mechanism; Cepeda supplies the schedule's shape. Almost no tool, Anki included, schedules against a deadline at all.

- **Source Connection:** FSRS DSR docs / srs-benchmark; Cepeda et al. (2008). _Caveat: the 20–30% figure is simulation-derived._
- **SPOV Connection:** → _Prospective SPOV: "Schedule backward from test day, not forward to infinity."_

## Theme D — Failure modes are diagnostic signal

### Insight 5: "Ease hell," leeches, and burnout aren't user error — they're the system mistaking a _card-quality_ problem for a _scheduling_ problem, which is the single best place to insert AI.

SM-2's response to repeated failure is to shrink intervals (the ease-hell spiral); Anki flags a "leech" only after 8 lapses, and its own manual attributes leeches to poorly written or interfering cards — with Wozniak naming interference "the single greatest cause of forgetting." So a chronically failing card is _diagnostic_: it's usually non-atomic, interfering with a sibling (amino acids, hormones, hydroxyl-vs-carbonyl), or simply not understood. The right response is to reformulate or re-teach — not reschedule harder. This converts Anki's most-hated failure mode into the app's highest-leverage intervention: detect the leech _early_, trigger an AI atomization/rewrite, and surface a "do you actually understand this?" teaching moment.

- **Source Connection:** SM-2 mechanics; Anki "Leeches" (8-lapse, interference); Wozniak's 20 Rules / minimum information principle.
- **SPOV Connection:** → _Prospective SPOV: "A leech is a feature, not a bug — it's the system asking to teach."_

## Theme E — AI: grounding is the bright line

### Insight 6: The wildly conflicting LLM hallucination numbers resolve into one design rule — AI may _transform_ vetted content but must never _originate_ facts.

The hallucination evidence looks contradictory until you sort it by grounding: ungrounded/adversarial medical generation fabricates in up to ~83% of cases and invents references freely, whereas source-grounded summarization runs ~1.47% (≈ human clinical notes), and RAG has driven hallucination to 0% vs. 8%. Generated MCQs compound the risk — GPT-4 items match human quality _on average_ but carry ~4–5% defect rates (multiple-correct, answer-giveaway) vs. ~1% for humans. The clean, defensible line: AI is safe for _source-grounded transformation_ (summarize this AAMC-aligned passage into atomic cards; write distractors from this verified answer; explain this correct solution) and unsafe for _ungrounded origination_ (recall a fact; judge whether a claim is true) — with mandatory QA gating before any generated item reaches a learner.

- **Source Connection:** Asgari et al. (2025, 1.47%); RAG studies (0% vs. 8%); Omar et al. (2025, ≤83%); Chelli et al. (2024); Doughty et al. (2024).
- **SPOV Connection:** → _Prospective SPOV: "AI is a grounded transformer, never an oracle."_

### Insight 7: AI tutoring's measurable payoff comes from feeding it the student's _state_, not from the model's eloquence — and the gains accrue to practice, not the chatbot.

Khan Academy's published results found a +6.1% improvement in _unaided next-item correctness_ specifically when the tutor was given structured signals about the student (recent history + unmastered prerequisites) — and they repeatedly stress that practice, not the AI feature, drives the learning. This reframes "AI tutor" away from a free-floating chat and toward a system wired into per-concept mastery and the knowledge graph. The implication: the tutor's value is proportional to how well it's grounded in (a) the correct answer and (b) the learner's mastery map — the same grounding principle as Insight 6, applied to dialogue instead of content.

- **Source Connection:** Khan Academy AI-tutor learnings (2025–26, +6.1% on unaided next-item correctness).
- **SPOV Connection:** → bridges the AI-grounding and knowledge-graph SPOVs.

## Theme F — Performance ≠ learning (the unifying error)

### Insight 8: Streaks, review counts, fluency, and self-rated confidence are the same trap in four costumes — all are _performance/engagement proxies_ that can move opposite to learning.

Bjork's storage-vs-retrieval framework holds that observable performance is an unreliable index of durable learning, and the "illusion of competence" makes fluent rereading _feel_ like mastery; Roediger & Karpicke showed restudy actively _inflates_ confidence while _worsening_ delayed retention; Duolingo's own critics note that streak-protection "activates the retention mechanism without the learning," with engagement diverging from learning at the edges. The unifying epiphany: bad gamification and the metacognitive illusion are the _same_ mistake — trusting a felt or counted proxy. The corollary, borrowed directly from Khan's metric choice, is to reward and display _delayed, unaided retrieval and transfer_ — never streaks, card volume, or confidence.

- **Source Connection:** Bjork & Bjork (desirable difficulties / New Theory of Disuse); Roediger & Karpicke (2006); Duolingo critiques; Khan Academy.
- **SPOV Connection:** → _Prospective SPOV: "Optimize learning, not engagement — and never confuse the two."_

## Theme G — One deck can't be right for everyone

### Insight 9: Expertise reversal makes a static premade deck structurally wrong — the _same_ hard card is a desirable difficulty for one student and damage for another, so difficulty must be titrated to per-concept mastery.

Kirschner, Sweller & Clark show novices need worked examples and explicit guidance, and that this advantage _reverses_ as expertise grows (the expertise-reversal effect, rooted in a ~4-chunk working memory). Bjork cautions that a difficulty is only _desirable_ if the learner has the prerequisites to meet it — otherwise it's just difficulty; Pan & Rickard show transfer ≈ 0 when initial retrieval fails. So an identical interleaved or application item _helps_ a prepared learner and _harms_ an unprepared one. The fix is Math Academy's model: a prerequisite knowledge graph with per-concept mastery gating, worked examples first, retrieval/interleaving once a threshold is crossed, and implicit "trickle-down" credit so advanced practice maintains the basics. This makes the knowledge graph a _correctness requirement_, not a nice-to-have — the substrate that lets the rest of the science be applied safely, per student.

- **Source Connection:** Kirschner/Sweller/Clark (2006); Bjork & Bjork; Pan & Rickard (2018); Skycak / Math Academy FIRe.
- **SPOV Connection:** → _Prospective SPOV: "Mastery-gated and graph-native beats a flat premade deck."_

## Theme H — Direction of flow: questions first

### Insight 10: Flashcards should sit _downstream_ of missed practice questions, not upstream of them — practice is the diagnostic engine; cards are the retention patch.

Jack Westin's instructor framing ("Anki is a do-not-forget-it tool, not a learn-it tool"; tie cards to passage mistakes) lines up precisely with the science: interleaved _practice_ forces the strategy-selection the exam tests (Rohrer, 61% vs. 38%), retrieval transfers best when it mirrors the criterion task (Pan & Rickard's response congruency), and AAMC full-lengths are the gold-standard readiness signal (third-party tests are deflated and unreliable at the extremes). Put together, the highest-yield loop is: practice → error analysis by type → _auto-generate an elaborated card from the gap_ → spaced review → re-test in passage form. Most Anki workflows run the arrow backwards (grind a premade deck, then hope it transfers). Inverting it makes content review demand-driven and keeps every card tethered to a real, observed weakness.

- **Source Connection:** Jack Westin; Rohrer et al. (2020); Pan & Rickard (2018); MCAT score-prediction sources (AAMC FLs).
- **SPOV Connection:** → _Prospective SPOV: "Practice is the engine; cards are the exhaust."_

---

# Experts to Follow

### Piotr Woźniak (SuperMemo)

- **Main views:** Invented computational spaced repetition (the SM-0 → SM-18 algorithm family, 1985–present) that Anki's SM-2 and, indirectly, FSRS descend from; co-authored the two-component model of memory (stability + retrievability, 1995) — the conceptual ancestor of FSRS's Difficulty-Stability-Retrievability model; the minimum information principle (cards should be as atomic as possible) and the "20 Rules of Formulating Knowledge"; interference is "probably the single greatest cause of forgetting" in a mature collection; cloze deletion is effective but shouldn't be overused; pioneer of incremental reading; skeptic of compulsory schooling in favor of an intrinsic "learn drive."
- **Why Follow:** The entire scheduling lineage your app sits on traces to him, and his stability/retrievability framing is literally the core of FSRS — so he's the primary source for reasoning about retention targets and why SM-2's ease model fails. His card-formulation rules are the most actionable existing spec for the app's _card-quality enforcement at creation_ (atomicity, interference detection), and his minimum-information principle directly underwrites the "fewer, higher-quality cards" stance. Note he has abandoned peer review and now publishes raw ideas on his wiki, so treat it as a brilliant primary practitioner source, not vetted literature.
- **Locations:**
	- Website / wiki: https://supermemo.guru (also super-memory.com, supermemo.com)
	- X/Twitter: @SuperMemoWoz
	- Affiliation: Founder, SuperMemo World / SuperMemo R&D (Poland)

### Jeffrey D. Karpicke (Purdue University)

- **Main views:** **Retrieval-based learning** — retrieval is "the key process for understanding and promoting learning," not merely assessment, and every act of retrieval _changes_ memory; retrieval practice outperforms elaborative study (e.g., concept mapping), including on inference/comprehension items (Karpicke & Blunt 2011, _Science_); students hold strong metacognitive illusions (they predict restudy/elaboration will beat testing — and are wrong); the adaptive value of forgetting; retrieval works via retrieval-specific mechanisms, not elaboration.
- **Why Follow:** Retrieval practice is the single highest-utility technique your app is built around, and Karpicke is its most prolific contemporary investigator (still publishing 2024–2025). His work is the evidentiary backbone for two design commitments: every content unit should _recur as retrieval rather than rereading_, and the app should _not trust learner-reported confidence_ as a mastery signal. His elementary-school and working-memory-individual-difference work also speaks to who benefits and when.
- **Locations:**
	- Lab website: https://learninglab.psych.purdue.edu/
	- Google Scholar: https://scholar.google.com/citations?user=5t5lgCgAAAAJ
	- Institution: Professor of Psychological Sciences, Purdue University

### Robert A. Bjork & Elizabeth L. Bjork(UCLA — Bjork Learning and Forgetting Lab)

- **Main views:** The learning-vs-performance distinction — what you observe during study (performance) is "an unreliable index" of durable learning; the New Theory of Disuse (independent _storage strength_ vs _retrieval strength_, with the largest storage gains coming from effortful retrieval when retrieval strength is low); desirable difficulties (spacing, interleaving, testing/generation, varying conditions); the illusion of competence — "being suspicious of the sense of ease"; forgetting as a friend of learning.
- **Why Follow:** They supply the theory that makes your most contrarian SPOVs defensible — _mastery ≠ lesson completion_, _engineer difficulty rather than fluency_, and _distrust streaks/recognition because ease is not learning_. The storage/retrieval framework is the right lens for setting FSRS desired-retention and for the critique of recognition-heavy, over-cloze'd cards that feel easy but don't transfer. Foundational and still active (desirable-difficulties work through 2019–2020).
- **Locations:**
	- Lab website: https://bjorklab.psych.ucla.edu/
	- Robert Bjork publications: https://bjorklab.psych.ucla.edu/robert-a-bjork-publications/
	- Institution: Department of Psychology, UCLA (Robert A. Bjork is Distinguished Research Professor; both are co-PIs of the lab)

### Doug Rohrer (University of South Florida)

- **Main views:** Interleaved practice improves mathematics/STEM learning by forcing students to _choose a strategy based on the problem itself_ (pairing each problem with the correct procedure) — demonstrated in classroom RCTs (e.g., 61% vs 38%, d=0.83; 77% vs 38%); spacing improves test scores and _reduces overconfidence_; both are "desirable difficulties"; students systematically underappreciate spacing and interleaving; prominent skeptic of "learning styles."
- **Why Follow:** Interleaving is the mechanism that bridges rote recall and the _application/strategy-selection_ the MCAT actually rewards — making Rohrer the key source for why the app must mix problem and passage _types_, not just shuffle facts. His preregistered classroom trials are the strongest real-world evidence that this transfers, and his free Interleaved Mathematics Practice Guide is effectively a ready-made design spec. Active through 2024–2025.
- **Locations:**
	- Personal page + publications (PDFs): http://uweb.cas.usf.edu/~drohrer/ (pubs: http://uweb.cas.usf.edu/~drohrer/pubs.htm)
	- Faculty page: https://www.usf.edu/arts-sciences/departments/psychology/people/faculty/doug-rohrer.aspx
	- Email: drohrer@usf.edu
	- Institution: Department of Psychology, University of South Florida (Tampa)

### Justin Skycak (Math Academy)

- **Main views:** Math/STEM is a hierarchical knowledge graph, not a pile of isolated facts; an effective system is an expert system over that graph combining mastery learning, spaced repetition, interleaving, and minimization of associative interference; FIRe (Fractional Implicit Repetition) — advanced practice gives discounted "trickle-down" review credit to prerequisite topics, reviews are chosen so their implicit repetitions "knock out" other due reviews like dominoes, and spacing speed is calibrated per student per topic; questions should be non-memorizable/regenerated; mastery means demonstrated, not "covered."
- **Why Follow:** Math Academy is the closest existing product to what you're building — adaptive, knowledge-graph-driven, spaced-repetition-based — and Skycak openly documents the underlying algorithms (in _The Math Academy Way_ and his blog). He's the direct source for the "your tags + prerequisites are a latent knowledge graph; add per-concept continuous mastery + implicit credit" insight and the "mastery ≠ completion" SPOV. Extremely active (blog, podcast, X). Caveat: this is a practitioner/company perspective — e.g., the "4× speed" figure is Math Academy's own marketing, not independent research.
- **Locations:**
	- Blog / link hub: https://www.justinmath.com/
	- LinkedIn: https://www.linkedin.com/in/justinskycak/
	- X/Twitter: linked from the justinmath.com link hub
	- Institution: Chief Quant / Director of Analytics, Math Academy

### Other

_Adjacent names worth a lighter follow, by pillar:_ **John Dunlosky** (Kent State) for periodic "what actually works" technique reviews; **Henry L. Roediger III** (Washington University) as Karpicke's frequent collaborator on the testing effect; **John Sweller** (UNSW) for cognitive load / worked examples that should precede open practice for novices; and the **open-spaced-repetition / FSRS** maintainers (Jarrett Ye and the srs-benchmark project on GitHub) for the live state of scheduling algorithms and benchmarks.

---

# DOK 2 Knowledge Tree

## Separate Files:

1. [[mcat structure]]
2. [[studying methods]]
3. [[test-enhance learning]]
4. [[fsrs]]
5. [[medical anki]]
6. [[ai generation]]

## F. AI tutoring

### F1. Khan Academy — Khanmigo & "Our Most Recent Learnings" (2025–2026)
*blog.khanacademy.org ("How Khan Academy Is Building a Better AI Tutor", "Learning in the Open")* · (product/methodology; **instructor/company**)
- GPT-4-based Socratic tutor; grounded in Khan's verified content library; guardrails prevent giving away answers.
- 2025–2026: ~20 product tests across 15M+ tutoring threads.
- +6.1% next-item correctness when the tutor has structured signals from the student's learning record (grounding in student state, not just better chat).
- Repeatedly stresses practice — not the AI feature — drives measured learning gains ("not a replacement for teachers or for practice").
- Names hallucination as "the main problem with this technology."
- **DOK 2 Summary:** AI's value is as a *thin, grounded layer over good structure*, not a freestanding tutor. Validates the brief's "ground AI in lesson state"/learner context.

## G. MCAT score prediction

### G1. AAMC FL predictiveness & third-party deflation (synthesis of practitioner sources)
*premiermcatprep.com; joel.vg (Joel Harris); residencyadvisor.com; elitemedicalprep.com* · (prep-company/practitioner analyses; **instructor/practitioner**)
- AAMC full-lengths are the gold-standard predictor (low bias, high correlation with the real exam); FL4/FL5 considered most representative.
- Average of the last two AAMC FLs is the most reliable point estimate.
- Less accurate at the extremes (especially 515+), where ceiling/compression distorts conversions.
- Third-party FLs are deflated with known offsets: Joel Harris's 354-point Blueprint dataset gives R²=0.536, Blueprint ≈ +2 to +7; Kaplan crudely ≈ +10.
- Implication: a readiness predictor should anchor on AAMC FLs, apply documented third-party adjustments, and display uncertainty bands rather than a single number.
- **DOK 2 Summary:** MCAT score very difficult to predict without AAMC FLs.

## H. Knowledge graphs, mastery & implicit review credit

### H1. Skycak / Math Academy — "Optimized, Individualized Spaced Repetition in Hierarchical Knowledge Structures" & "Cognitive Science of Learning: Spaced Repetition"
*justinmath.com; The Math Academy Way* · (practitioner/theory; **researcher/practitioner**)
- Treats a STEM curriculum as a connected prerequisite knowledge graph, not independent flashcards.
- Fractional Implicit Repetition (FIRe), a proprietary model layering spaced repetition onto a prerequisite knowledge graph of thousands of math topics:
	1. advanced practice gives discounted "trickle-down" repetition credit to prerequisite topics;
	2. review selection is optimized so each review's implicit repetitions knock out other due reviews ("repetition compression," ~dominoes);
	3. spacing speed is calibrated per student per topic (ability × difficulty).
- Built on mastery learning (gate advancement on prerequisite mastery) using non-memorizable, regenerated questions to prevent answer-pattern memorization.
- A student's knowledge profile = accumulated spaced repetitions per topic; also leverages layering, interleaving, and minimizing associative interference.
- Directly portable: map MCAT content to a prerequisite graph; grant implicit credit when a passage exercises multiple sub-concepts; define mastery as delayed/interleaved/transfer performance, not card completion.

## I. Instructor MCAT methodology

### I1. Jack Westin — "How to Use Anki Effectively for the MCAT (Without Wasting Hours)"
*jackwestin.com/blog* · (prep company; **instructor**)
- Anki is a "do-not-forget-it" tool, NOT a "learn-it" tool — use it *after* you understand the material.
- Make fewer, higher-quality cards; tie cards to passage mistakes rather than pre-making thousands.
- Memorable framing: "Practice finds weaknesses. Anki prevents re-forgetting."
- As a CARS-focused company, emphasizes CARS is pure reasoning with no memorizable content — flashcards can't cover it.

### I2. MedLife Mastery — "How to Use Anki for the MCAT (517 Scorer Strategy)"
*medlifemastery.com* · (prep company/scorer; **instructor/student**)
- Documents a mentor going 499→517 in ~6 weeks with a disciplined Anki system layered on practice.
- Recommends reviewing the entire mixed deck (interleaving) rather than topic-blocked study.
- Advocates image occlusion for diagrams, pathways, and psych/soc concepts.

### I3. MCAT prep consensus pattern (Kaplan / Khan Academy / UWorld / AAMC)
*aggregated company guidance & how-to guides (mcat.tools, coconote, JackWestin)* · (prep companies; **instructor**)
- Standard arc: Phase 1 content review → Phase 2 practice (UWorld question blocks + daily CARS) → Phase 3 weekly AAMC full-lengths.
- Journal every missed question by error type (content gap vs misreading vs timing vs careless) and convert genuine content gaps into cards.
- The workflow is practice-driven, not content-driven — flashcards serve practice, not vice versa.

## J. Student / community

### J1. r/MCAT, r/AnkiMCAT, Student Doctor Network threads
*reddit.com/r/MCAT, r/AnkiMCAT; forums.studentdoctor.net* · (forums; **student**)
- Dominant premade decks: AnKing, MileDown, JackSparrow, MrPankow (P/S), Benoni (C/P) — the "deck wars."
- Recurring scorer accounts (e.g., 508→520 with MileDown + practice tests as the only study) — flashcards plus heavy practice, not flashcards alone.
- Strong consensus to use premade decks as raw material, edit aggressively, and not feel obligated to finish all ~5,000 cards.
- Frequent complaints: review pile-ups, burnout, ease hell, and time lost to Anki vs passages — the lived version of the documented failure modes.

## K. Gamification & motivation

### K1. Duolingo gamification analyses (engagement vs learning)
*dev.to ("Why Duolingo's Gamification Works (And When It Doesn't)"); Shortt et al. 2023 systematic review; uladshauchenka.com* · (case studies/critique; **mixed: practitioner/researcher**)
- **dev.to ("Why Duolingo's Gamification Works (And When It Doesn't)"):**
	- The daily streak is described as the most powerful retention tool, working via loss aversion — the magnitude of which traces to Tversky & Kahneman (1992), "Advances in Prospect Theory," loss-aversion coefficient λ ≈ 2.25 ("losses loom more than twice as large as equivalent gains," from Kahneman & Tversky's 1979 prospect theory).
	- Streak-protection/speed-running critique: "Speed-running an easy lesson to protect a streak activates the retention mechanism without the learning."
	- Engagement metrics (DAU, session length, streak continuation) align with learning outcomes at the center but diverge at the edges; the difficulty algorithm is "historically not aggressive enough" because harder material lowers engagement.
	- Overjustification effect: streak-dependent learners abandon more readily when the streak breaks.
- **Shortt, Tilak, Kuznetcova, Martens & Akinkuolie (2023), *Computer Assisted Language Learning* 36(3):517–554:**
	- Systematic review of Duolingo literature from public release (2012) to early 2020. Most studies examined engagement/design rather than learning outcomes; where learning was measured, gains were mainly in vocabulary.
	- Gamification elements (badges, leaderboards, streaks, points) boost initial engagement but struggle with advanced skills/practical application.
	- Desirable-difficulty critique: once novelty wears off, gamification "cannot compensate for design decisions prioritizing competition over collaboration, repetition and translation over meaningful feedback and context"; the review recommends collaboration + meaningful feedback over competition + repetition.
- **DOK 2 Summary:** Gamification tends to maximize *engagement* (performance) over actual learning — which are often inversely correlated.
