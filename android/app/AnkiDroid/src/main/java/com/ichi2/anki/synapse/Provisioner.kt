/*
 * Copyright (c) 2026 Synapse contributors
 *
 * This program is free software; you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation; either version 3 of the License, or (at your option) any later
 * version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
 * PARTICULAR PURPOSE. See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program. If not, see <http://www.gnu.org/licenses/>.
 */
package com.ichi2.anki.synapse

import anki.deck_config.DeckConfig
import anki.deck_config.copy
import anki.deck_config.deckConfig
import anki.deck_config.updateDeckConfigsRequest
import com.ichi2.anki.libanki.Collection
import com.ichi2.anki.libanki.DeckConfigId
import com.ichi2.anki.libanki.DeckId
import com.ichi2.anki.libanki.NoteTypeId
import timber.log.Timber

/**
 * Synapse demo-environment provisioning for AnkiDroid.
 *
 * A faithful Kotlin port of the desktop `qt/aqt/synapse/provision.py`. It builds
 * a small, self-contained Synapse demo inside an existing collection: it turns
 * on FSRS, creates a dedicated "Synapse" deck-config preset with non-blocked
 * (randomised) ordering, defines the MCAT notetypes, creates a "Synapse" deck
 * bound to the preset, and seeds a handful of concept-tagged demo notes.
 *
 * These steps activate the (already-compiled-in) core scheduler features which
 * are otherwise dormant.
 *
 * Design constraint (mirroring the desktop module): every function operates on
 * an open [Collection]. There is no Android/UI dependency here so the logic can
 * be exercised directly and headless.
 *
 * Non-obvious backend behaviours worth calling out:
 *
 * * FSRS is a *collection-global* boolean. It cannot be flipped with a plain
 *   config set; it is only persisted through the deck-options update flow
 *   ([enableFsrsAndPreset]).
 * * In that same flow the *target deck* is (re)bound to whichever config is
 *   `configs.last()`. We make the "Synapse" preset the last config so nothing
 *   else is disturbed. The Synapse deck is bound to the preset separately in
 *   [createSynapseDeck], keeping the two concerns independent and idempotent.
 * * Concept tags follow the convention `concept::<section>::<id>` where every
 *   segment uses underscores and contains no spaces (Anki splits tags on
 *   whitespace, so a space would create two tags).
 *
 * All shared names/constants come from [Synapse]; do not redefine them here.
 */
object Provisioner {
    // --- Options -------------------------------------------------------------

    /**
     * Which Synapse features to enable when provisioning. Defaults reproduce the
     * recommended Synapse behaviour, matching desktop `SynapseOptions()`.
     *
     * * [enableFsrs] — turn on collection-global FSRS via the deck-options flow.
     *   (Provisioning never turns FSRS *off*; unchecking simply won't enable it.)
     * * [interleaveByConcept] / [masteryGating] / [trickleDownCredit] /
     *   [metamorphosis] — the per-preset scheduling flags.
     * * [adoptionEnabled] — the `synapse:adoption_enabled` generic config that
     *   drives the adoption ("Effort") stats panel.
     * * [governorEnabled] + [testDate] — the `synapse:governor_enabled` /
     *   `synapse:test_date` config that drives the exam-date retention governor.
     *   [testDate] is an ISO `"YYYY-MM-DD"` string (or `null`). The governor keys
     *   are only written when [governorEnabled] is true.
     * * [installSeedContent] — whether to seed the demo notes (both the MCAT
     *   Application notes and the item-notetype notes).
     * * [itemNotetypes] — the set of item-notetype display names to create
     *   (a subset of [ITEM_NOTETYPES]' names).
     */
    data class SynapseOptions(
        val enableFsrs: Boolean = true,
        val interleaveByConcept: Boolean = true,
        val masteryGating: Boolean = true,
        val trickleDownCredit: Boolean = true,
        val metamorphosis: Boolean = true,
        val adoptionEnabled: Boolean = true,
        val governorEnabled: Boolean = false,
        val testDate: String? = null,
        val installSeedContent: Boolean = true,
        val itemNotetypes: Set<String> = ITEM_NOTETYPES.map { it.name }.toSet(),
    )

    /** Summary of what a provisioning run created/looked up. */
    data class ProvisionResult(
        val notetypeId: NoteTypeId,
        val itemNotetypeIds: Map<String, NoteTypeId>,
        val deckId: DeckId,
        val configId: DeckConfigId,
        val notesAdded: Int,
        val itemNotesAdded: Int,
        val fsrs: Boolean,
    ) {
        /** Total notetypes created/reused (MCAT Application + item notetypes). */
        val notetypesCount: Int get() = 1 + itemNotetypeIds.size

        /** Total demo notes seeded across all notetypes. */
        val totalNotesAdded: Int get() = notesAdded + itemNotesAdded
    }

    // --- Notetype specs ------------------------------------------------------

    /** Front/back template pair for a card. */
    private data class Template(
        val qfmt: String,
        val afmt: String,
    )

    /** Declarative description of an application-item notetype. */
    private data class ItemNotetypeSpec(
        val name: String,
        val fields: List<String>,
        val sortField: String,
        val template: Template,
    )

    // The "MCAT Application" notetype's card template. Front: passage, stem, then
    // a typed answer box. Back: the front, a divider, the answer + explanation,
    // then a conditional cited-source line (only shown when Grounding is filled).
    private val MCAT_TEMPLATE =
        Template(
            qfmt =
                """
                <div class="passage">{{Passage}}</div>
                <hr class="passage-divider">
                <div class="stem">{{Stem}}</div>
                {{type:Answer}}
                """.trimIndent(),
            afmt =
                """
                {{FrontSide}}
                <hr id="answer">
                <div class="answer">{{Answer}}</div>
                <div class="explanation">{{Explanation}}</div>
                {{#Grounding}}<div class="grounding">Source: {{Grounding}}</div>{{/Grounding}}
                """.trimIndent(),
        )

    // The three richer M1 application-item notetypes (decision #4): each is a new
    // notetype + template only, no AI grading. Template conventions match MCAT
    // Application: {{type:<Field>}} renders a typed-answer comparison box where a
    // short checkable answer fits; Explain-Why has no single correct answer so it
    // self-grades via a reveal. The back always starts with {{FrontSide}} then an
    // <hr id="answer">.
    private val ITEM_NOTETYPES: List<ItemNotetypeSpec> =
        listOf(
            // "Which principle/concept applies?"
            ItemNotetypeSpec(
                name = Synapse.WHICH_PRINCIPLE_NOTETYPE_NAME,
                fields = listOf("Stem", "Options", "Answer", "Explanation", "Concept"),
                sortField = "Stem",
                template =
                    Template(
                        qfmt =
                            """
                            <div class="stem">{{Stem}}</div>
                            <div class="options">{{Options}}</div>
                            <div class="prompt">Which principle applies?</div>
                            {{type:Answer}}
                            """.trimIndent(),
                        afmt =
                            """
                            {{FrontSide}}
                            <hr id="answer">
                            <div class="answer">{{Answer}}</div>
                            <div class="explanation">{{Explanation}}</div>
                            """.trimIndent(),
                    ),
            ),
            // A small data table / figure description followed by a question.
            ItemNotetypeSpec(
                name = Synapse.DATA_SNIPPET_NOTETYPE_NAME,
                fields = listOf("Data", "Question", "Answer", "Explanation", "Concept"),
                sortField = "Question",
                template =
                    Template(
                        qfmt =
                            """
                            <div class="data">{{Data}}</div>
                            <hr class="data-divider">
                            <div class="question">{{Question}}</div>
                            {{type:Answer}}
                            """.trimIndent(),
                        afmt =
                            """
                            {{FrontSide}}
                            <hr id="answer">
                            <div class="answer">{{Answer}}</div>
                            <div class="explanation">{{Explanation}}</div>
                            """.trimIndent(),
                    ),
            ),
            // An open "explain why" prompt whose back reveals a model explanation.
            ItemNotetypeSpec(
                name = Synapse.EXPLAIN_WHY_NOTETYPE_NAME,
                fields = listOf("Prompt", Synapse.FIELD_MODEL_ANSWER, "Concept"),
                sortField = "Prompt",
                template =
                    Template(
                        qfmt =
                            """
                            <div class="prompt">{{Prompt}}</div>
                            <div class="hint">Explain your reasoning, then reveal the model answer.</div>
                            """.trimIndent(),
                        afmt =
                            """
                            {{FrontSide}}
                            <hr id="answer">
                            <div class="model-answer">{{ModelAnswer}}</div>
                            """.trimIndent(),
                    ),
            ),
        )

    // --- Seed data -----------------------------------------------------------

    /** One MCAT Application demo note (concept-tagged). */
    private data class SeedNote(
        val tag: String,
        val concept: String,
        val stem: String,
        val answer: String,
        val explanation: String,
        val passage: String = "",
    )

    // Edit this list to change the demo content. `concept` is the human-readable
    // concept name (stored in the Concept field); `tag` is the machine concept
    // tag (concept::<section>::<id>) attached as the note's single tag.
    private val SEED_NOTES: List<SeedNote> =
        listOf(
            // --- Biochem: amino acid charge ---
            SeedNote(
                tag = "concept::biochem::amino_acid_charge",
                concept = "Amino acid charge",
                stem = "At physiological pH (7.4), what is the net charge of a lysine side chain in a peptide?",
                answer = "Positive",
                explanation =
                    "Lysine's epsilon-amino group has a pKa around 10.5, so at pH 7.4 it stays protonated and carries a +1 charge.",
            ),
            SeedNote(
                tag = "concept::biochem::amino_acid_charge",
                concept = "Amino acid charge",
                stem =
                    "An enzyme is most active at pH 3. Which residue is most likely protonated and contributing a positive charge there?",
                answer = "Histidine",
                explanation =
                    "Histidine's imidazole (pKa ~6) is protonated below pH 6, making it the residue whose charge shifts most in acidic conditions.",
            ),
            SeedNote(
                tag = "concept::biochem::amino_acid_charge",
                concept = "Amino acid charge",
                stem = "During isoelectric focusing a protein stops migrating at pH 5.2. What does this pH represent?",
                answer = "Its pI",
                explanation =
                    "At the isoelectric point (pI) the protein's net charge is zero, so it no longer moves in the electric field.",
            ),
            // --- Biochem: enzyme kinetics ---
            SeedNote(
                tag = "concept::biochem::enzyme_kinetics",
                concept = "Enzyme kinetics",
                stem = "A competitive inhibitor is added to an enzyme assay. How does the apparent Km change?",
                answer = "It increases",
                explanation =
                    "Competitive inhibitors raise the apparent Km (weaker apparent affinity) while leaving Vmax unchanged.",
            ),
            SeedNote(
                tag = "concept::biochem::enzyme_kinetics",
                concept = "Enzyme kinetics",
                stem = "On a Lineweaver-Burk plot a noncompetitive inhibitor shifts the y-intercept upward. What does that indicate?",
                answer = "Lower Vmax",
                explanation =
                    "The y-intercept equals 1/Vmax, so a higher intercept means Vmax has decreased, the signature of noncompetitive inhibition.",
            ),
            SeedNote(
                tag = "concept::biochem::enzyme_kinetics",
                concept = "Enzyme kinetics",
                stem = "Substrate concentration is far above Km. What is the approximate reaction rate relative to Vmax?",
                answer = "Near Vmax",
                explanation =
                    "When [S] >> Km the enzyme is saturated, so the rate approaches Vmax and is nearly independent of substrate concentration.",
            ),
            // --- Physics: circuits / Ohm's law ---
            SeedNote(
                tag = "concept::physics::circuits_ohms_law",
                concept = "Ohm's law",
                stem = "A 12 V battery drives 3 A through a resistor. What is the resistance?",
                answer = "4 ohms",
                explanation = "By Ohm's law R = V/I = 12 V / 3 A = 4 ohms.",
            ),
            SeedNote(
                tag = "concept::physics::circuits_ohms_law",
                concept = "Ohm's law",
                stem = "Two identical resistors are placed in parallel. How does the total resistance compare to one resistor alone?",
                answer = "Half",
                explanation = "Equal resistors in parallel combine to R/2, so the total resistance is half that of a single resistor.",
            ),
            SeedNote(
                tag = "concept::physics::circuits_ohms_law",
                concept = "Ohm's law",
                stem = "A resistor dissipates 6 W while carrying 2 A. What voltage is across it?",
                answer = "3 V",
                explanation = "Power P = IV, so V = P/I = 6 W / 2 A = 3 V.",
            ),
            // --- Psych: operant conditioning ---
            SeedNote(
                tag = "concept::psych::operant_conditioning",
                concept = "Operant conditioning",
                stem = "A rat's lever-pressing increases after a shock is removed each time it presses. What process is this?",
                answer = "Negative reinforcement",
                explanation = "Removing an aversive stimulus (the shock) to increase a behavior is negative reinforcement.",
            ),
            SeedNote(
                tag = "concept::psych::operant_conditioning",
                concept = "Operant conditioning",
                stem = "A slot machine pays out after an unpredictable number of pulls. Which reinforcement schedule is this?",
                answer = "Variable ratio",
                explanation =
                    "Reward after an unpredictable number of responses is a variable-ratio schedule, which yields high, steady responding.",
            ),
            SeedNote(
                tag = "concept::psych::operant_conditioning",
                concept = "Operant conditioning",
                stem = "A child stops throwing tantrums after losing screen time for each outburst. What process reduced the behavior?",
                answer = "Negative punishment",
                explanation = "Removing a desirable stimulus (screen time) to decrease a behavior is negative punishment.",
            ),
        )

    /** One demo note for an item notetype: a single tag + field->value map. */
    private data class ItemSeedNote(
        val tag: String,
        val fields: Map<String, String>,
    )

    // Demo notes for the M1 item notetypes, keyed by notetype display name. Each
    // note's `fields` map covers that notetype's exact field set (see
    // ITEM_NOTETYPES). Seeded idempotently by [seedItemNotes].
    private val ITEM_SEED_NOTES: Map<String, List<ItemSeedNote>> =
        mapOf(
            Synapse.WHICH_PRINCIPLE_NOTETYPE_NAME to
                listOf(
                    ItemSeedNote(
                        tag = "concept::biochem::enzyme_kinetics",
                        fields =
                            mapOf(
                                "Stem" to "Doubling [substrate] far below Km roughly doubles the initial reaction rate.",
                                "Options" to "First-order kinetics; Zero-order kinetics; Competitive inhibition; Cooperativity",
                                "Answer" to "First-order kinetics",
                                "Explanation" to
                                    "Well below Km the rate is approximately proportional to [S], i.e. first-order in substrate.",
                                "Concept" to "Enzyme kinetics",
                            ),
                    ),
                    ItemSeedNote(
                        tag = "concept::physics::circuits_ohms_law",
                        fields =
                            mapOf(
                                "Stem" to "A wire's current rises linearly as the voltage across it increases, at constant temperature.",
                                "Options" to "Ohm's law; Kirchhoff's current law; Faraday's law; Coulomb's law",
                                "Answer" to "Ohm's law",
                                "Explanation" to
                                    "A linear V-I relationship at fixed temperature is the defining behavior of an ohmic resistor (V = IR).",
                                "Concept" to "Ohm's law",
                            ),
                    ),
                ),
            Synapse.DATA_SNIPPET_NOTETYPE_NAME to
                listOf(
                    ItemSeedNote(
                        tag = "concept::biochem::enzyme_kinetics",
                        fields =
                            mapOf(
                                "Data" to "Assay | Vmax | Km<br>Control | 100 | 5<br>+Drug X | 100 | 15",
                                "Question" to "What kind of inhibitor is Drug X?",
                                "Answer" to "Competitive",
                                "Explanation" to "Km rises while Vmax is unchanged, the signature of a competitive inhibitor.",
                                "Concept" to "Enzyme kinetics",
                            ),
                    ),
                    ItemSeedNote(
                        tag = "concept::physics::circuits_ohms_law",
                        fields =
                            mapOf(
                                "Data" to "V (V) | I (A)<br>2 | 0.5<br>4 | 1.0<br>6 | 1.5",
                                "Question" to "What is the resistance of this component?",
                                "Answer" to "4 ohms",
                                "Explanation" to "The V-I ratio is constant at 4 (e.g. 4 V / 1.0 A), so R = 4 ohms.",
                                "Concept" to "Ohm's law",
                            ),
                    ),
                ),
            Synapse.EXPLAIN_WHY_NOTETYPE_NAME to
                listOf(
                    ItemSeedNote(
                        tag = "concept::biochem::amino_acid_charge",
                        fields =
                            mapOf(
                                "Prompt" to "Explain why glycine has no net charge at its pI but lysine's pI is well above 7.",
                                Synapse.FIELD_MODEL_ANSWER to
                                    "At the pI the net charge is zero. Glycine has only its alpha-amino and alpha-carboxyl " +
                                    "groups, so its pI sits near neutral pH. Lysine adds a basic side chain (pKa ~10.5) that " +
                                    "must also be deprotonated to reach net zero, pushing its pI higher.",
                                "Concept" to "Amino acid charge",
                            ),
                    ),
                    ItemSeedNote(
                        tag = "concept::psych::operant_conditioning",
                        fields =
                            mapOf(
                                "Prompt" to
                                    "Explain why a variable-ratio schedule produces behavior that is especially resistant to extinction.",
                                Synapse.FIELD_MODEL_ANSWER to
                                    "Because reinforcement arrives after an unpredictable number of responses, the learner " +
                                    "cannot tell a temporary run of non-reward from true extinction, so responding persists " +
                                    "far longer than under predictable schedules.",
                                "Concept" to "Operant conditioning",
                            ),
                    ),
                ),
        )

    // --- Orchestration -------------------------------------------------------

    /**
     * Provision the full Synapse demo environment with the recommended defaults.
     * Idempotent: re-running is safe.
     */
    fun provision(col: Collection): ProvisionResult = provisionWithOptions(col, SynapseOptions())

    /**
     * Provision the Synapse environment according to [opts]. Idempotent.
     * Each flag on [opts] gates exactly one effect (see [SynapseOptions]).
     */
    fun provisionWithOptions(
        col: Collection,
        opts: SynapseOptions,
    ): ProvisionResult {
        Timber.i("Provisioning Synapse environment (fsrs=%b, seed=%b)", opts.enableFsrs, opts.installSeedContent)
        val configId = enableFsrsAndPreset(col, opts)
        val notetypeId = createMcatNotetype(col)

        // Only build the item notetypes the caller asked for (default: all).
        val itemSpecs = ITEM_NOTETYPES.filter { it.name in opts.itemNotetypes }
        val itemNotetypeIds = itemSpecs.associate { it.name to createItemNotetype(col, it) }

        val deckId = createSynapseDeck(col, configId)

        // Seed demo content only when requested. Seeding is itself idempotent, so
        // skipping it never leaves a half-populated deck.
        val notesAdded: Int
        val itemNotesAdded: Int
        if (opts.installSeedContent) {
            notesAdded = seedNotes(col, notetypeId, deckId)
            itemNotesAdded = seedItemNotes(col, itemNotetypeIds, deckId)
        } else {
            notesAdded = 0
            itemNotesAdded = 0
        }

        // M3-E: the adoption "Effort" panel (points + streak; generic collection
        // config read by stats::adoption).
        col.config.set(Synapse.CONFIG_ADOPTION_ENABLED, opts.adoptionEnabled)

        // A2: the test-date retention governor. Only written when enabled, so a
        // default provision leaves the governor untouched (opt-in). A blank/absent
        // date leaves the switch off, matching the "Set Exam Date..." action.
        if (opts.governorEnabled && !opts.testDate.isNullOrEmpty()) {
            col.config.set(Synapse.CONFIG_GOVERNOR_ENABLED, true)
            col.config.set(Synapse.CONFIG_TEST_DATE, opts.testDate)
        } else if (opts.governorEnabled) {
            // Enabled but no date supplied: nothing to schedule against, so keep
            // the governor off rather than write a switch with no date.
            col.config.set(Synapse.CONFIG_GOVERNOR_ENABLED, false)
        }

        return ProvisionResult(
            notetypeId = notetypeId,
            itemNotetypeIds = itemNotetypeIds,
            deckId = deckId,
            configId = configId,
            notesAdded = notesAdded,
            itemNotesAdded = itemNotesAdded,
            fsrs = opts.enableFsrs,
        )
    }

    /**
     * True once the Synapse environment exists (presence of the MCAT Application
     * notetype is our provisioned marker). Mirrors desktop `is_provisioned`.
     */
    fun isProvisioned(col: Collection): Boolean = col.notetypes.byName(Synapse.MCAT_NOTETYPE_NAME) != null

    // --- Step 1: FSRS + Synapse preset --------------------------------------

    /**
     * Ensure the "Synapse" preset exists, optionally enabling collection FSRS.
     * Returns the config id of the "Synapse" preset.
     *
     * FSRS is only persisted through the deck-options update flow, so we route
     * everything through [Decks.updateDeckConfigs][com.ichi2.anki.libanki.Decks.updateDeckConfigs].
     * In that flow the *target deck* is rebound to `configs.last()`; here the
     * target is simply the current deck (we only care about the global fsrs flag
     * + creating the preset), and we make the preset the last config so nothing
     * else is disturbed. The Synapse deck is bound to the preset separately in
     * [createSynapseDeck].
     */
    private fun enableFsrsAndPreset(
        col: Collection,
        opts: SynapseOptions,
    ): DeckConfigId {
        val did = col.decks.getCurrentId()
        val fu = col.decks.getDeckConfigsForUpdate(did)

        // Reuse the preset by name if it already exists (idempotent), otherwise
        // start from the backend defaults so all required numeric fields (learn
        // steps, per-day limits, valid FSRS params, ...) are populated. A fresh
        // DeckConfig with id==0 gets a new id assigned by the backend on save.
        val existing = fu.allConfigList.firstOrNull { it.config.name == Synapse.PRESET_NAME }?.config
        val base =
            existing ?: deckConfig {
                id = 0
                name = Synapse.PRESET_NAME
                config = fu.defaults.config
            }
        val preset = base.copy { config = applySynapseConfig(base.config, opts) }

        // FSRS is collection-global. When re-running with FSRS unchecked we must
        // not clobber an already-on collection: OR the request in with the
        // current state so provisioning never turns FSRS *off*.
        val fsrs = opts.enableFsrs || fu.fsrs

        // Configs list: send only what we touch. Keep the preset last so, per the
        // backend contract, the target deck ends up on it (harmless here).
        val req =
            updateDeckConfigsRequest {
                targetDeckId = did
                configs += preset
                cardStateCustomizer = fu.cardStateCustomizer
                newCardsIgnoreReviewLimit = fu.newCardsIgnoreReviewLimit
                this.fsrs = fsrs // collection-global; only takes effect via this flow
                applyAllParentLimits = fu.applyAllParentLimits
                fsrsReschedule = false
                fsrsHealthCheck = false
            }
        col.decks.updateDeckConfigs(req)

        // Re-read to obtain the (possibly newly assigned) preset id.
        return presetId(col)
    }

    /**
     * Return a copy of [config] with the Synapse-specific ordering + retention
     * applied. Each feature flag is written explicitly from [opts] (rather than
     * only switched on) so that re-provisioning with a feature *unchecked*
     * actually turns it back off on the preset.
     */
    private fun applySynapseConfig(
        config: DeckConfig.Config,
        opts: SynapseOptions,
    ): DeckConfig.Config =
        config.copy {
            // RANDOM gather/sort/review ordering avoids subject-blocking so mixed
            // concepts interleave.
            newCardGatherPriority = DeckConfig.Config.NewCardGatherPriority.NEW_CARD_GATHER_PRIORITY_RANDOM_NOTES
            newCardSortOrder = DeckConfig.Config.NewCardSortOrder.NEW_CARD_SORT_ORDER_RANDOM_NOTE_THEN_TEMPLATE
            reviewOrder = DeckConfig.Config.ReviewCardOrder.REVIEW_CARD_ORDER_RANDOM
            desiredRetention = Synapse.DESIRED_RETENTION.toFloat()
            // M1-B: concept + question-type interleaving. Default OFF globally.
            interleaveByConcept = opts.interleaveByConcept
            // M2: graph-driven adaptive scheduling + card metamorphosis, all
            // default OFF globally; Synapse turns them on.
            masteryGating = opts.masteryGating
            trickleDownCredit = opts.trickleDownCredit
            metamorphosis = opts.metamorphosis
        }

    /** Look up the Synapse preset id by name (after it has been saved). */
    private fun presetId(col: Collection): DeckConfigId {
        val fu = col.decks.getDeckConfigsForUpdate(col.decks.getCurrentId())
        return fu.allConfigList
            .firstOrNull { it.config.name == Synapse.PRESET_NAME }
            ?.config
            ?.id
            ?: throw IllegalStateException("Synapse preset not found after creation")
    }

    // --- Step 2: MCAT notetypes ---------------------------------------------

    /** Create (or reuse) the "MCAT Application" notetype. Returns its id. */
    private fun createMcatNotetype(col: Collection): NoteTypeId {
        col.notetypes.byName(Synapse.MCAT_NOTETYPE_NAME)?.let { return it.id }

        val nt = col.notetypes.new(Synapse.MCAT_NOTETYPE_NAME)
        for (fieldName in Synapse.MCAT_FIELDS) {
            col.notetypes.addField(nt, col.notetypes.newField(fieldName))
        }
        // Sort on Stem (the human-facing question).
        val sortIndex = Synapse.MCAT_FIELDS.indexOf(Synapse.FIELD_STEM)
        if (sortIndex >= 0) {
            col.notetypes.setSortIndex(nt, sortIndex)
        }
        val template =
            col.notetypes.newTemplate("Card 1").apply {
                qfmt = MCAT_TEMPLATE.qfmt
                afmt = MCAT_TEMPLATE.afmt
            }
        col.notetypes.add_template(nt, template)

        col.notetypes.add(nt)
        return nt.id
    }

    /**
     * Create (or reuse) an application-item notetype from its spec. Idempotent:
     * if a notetype with the spec's name already exists we return its id
     * untouched (we never mutate an existing notetype's fields/templates, which
     * would be a destructive schema change for any notes already using it).
     */
    private fun createItemNotetype(
        col: Collection,
        spec: ItemNotetypeSpec,
    ): NoteTypeId {
        col.notetypes.byName(spec.name)?.let { return it.id }

        val nt = col.notetypes.new(spec.name)
        for (fieldName in spec.fields) {
            col.notetypes.addField(nt, col.notetypes.newField(fieldName))
        }
        val sortIndex = spec.fields.indexOf(spec.sortField)
        if (sortIndex >= 0) {
            col.notetypes.setSortIndex(nt, sortIndex)
        }
        val template =
            col.notetypes.newTemplate("Card 1").apply {
                qfmt = spec.template.qfmt
                afmt = spec.template.afmt
            }
        col.notetypes.add_template(nt, template)

        col.notetypes.add(nt)
        return nt.id
    }

    // --- Step 3: Synapse deck -----------------------------------------------

    /** Create (or reuse) the "Synapse" deck and bind it to the preset. */
    private fun createSynapseDeck(
        col: Collection,
        presetId: DeckConfigId,
    ): DeckId {
        val did = col.decks.id(Synapse.DECK_NAME) // creates when missing

        // Bind the deck to the Synapse preset (idempotent: setConfigIdForDeckDict
        // just rewrites the deck dict's "conf").
        val deck = col.decks.getLegacy(did) ?: throw IllegalStateException("Synapse deck missing after creation")
        if (deck.conf != presetId) {
            col.decks.setConfigIdForDeckDict(deck, presetId)
        }
        return did
    }

    // --- Step 4: seed notes -------------------------------------------------

    /**
     * Seed the demo MCAT notes. Idempotent. Returns the number added.
     *
     * Guard: if the Synapse deck already contains any MCAT Application notes we
     * skip seeding entirely rather than risk duplicates.
     */
    private fun seedNotes(
        col: Collection,
        notetypeId: NoteTypeId,
        deckId: DeckId,
    ): Int {
        if (col.findNotes(deckNotetypeQuery(Synapse.MCAT_NOTETYPE_NAME)).isNotEmpty()) {
            return 0
        }
        val notetype = col.notetypes.get(notetypeId) ?: return 0

        var added = 0
        for (spec in SEED_NOTES) {
            val note = col.newNote(notetype)
            note.setItem(Synapse.FIELD_STEM, spec.stem)
            note.setItem(Synapse.FIELD_PASSAGE, spec.passage)
            note.setItem(Synapse.FIELD_ANSWER, spec.answer)
            note.setItem(Synapse.FIELD_EXPLANATION, spec.explanation)
            note.setItem(Synapse.FIELD_CONCEPT, spec.concept)
            // Exactly one concept tag per note; segments are underscore-joined and
            // space-free so Anki treats it as a single tag.
            note.tags = mutableListOf(spec.tag)
            col.addNote(note, deckId)
            added++
        }
        return added
    }

    /**
     * Seed demo notes for the M1 item notetypes. Idempotent. Returns count.
     *
     * Per notetype: skip seeding if the Synapse deck already holds any note of
     * that notetype (so re-provisioning never duplicates). Only fields declared
     * on the notetype are written.
     */
    private fun seedItemNotes(
        col: Collection,
        notetypeIds: Map<String, NoteTypeId>,
        deckId: DeckId,
    ): Int {
        var added = 0
        for ((name, specs) in ITEM_SEED_NOTES) {
            val notetypeId = notetypeIds[name] ?: continue
            // Per-notetype guard so a partially-seeded deck still fills the rest.
            if (col.findNotes(deckNotetypeQuery(name)).isNotEmpty()) {
                continue
            }
            val notetype = col.notetypes.get(notetypeId) ?: continue
            val fieldNames = notetype.fields.map { it.name }.toSet()

            for (spec in specs) {
                val note = col.newNote(notetype)
                for ((fieldName, value) in spec.fields) {
                    if (fieldName in fieldNames) {
                        note.setItem(fieldName, value)
                    }
                }
                note.tags = mutableListOf(spec.tag)
                col.addNote(note, deckId)
                added++
            }
        }
        return added
    }

    /**
     * Search string matching notes of [notetypeName] inside the Synapse deck,
     * e.g. `deck:Synapse note:"MCAT Application"`.
     */
    private fun deckNotetypeQuery(notetypeName: String): String = """deck:${Synapse.DECK_NAME} note:"$notetypeName""""
}
