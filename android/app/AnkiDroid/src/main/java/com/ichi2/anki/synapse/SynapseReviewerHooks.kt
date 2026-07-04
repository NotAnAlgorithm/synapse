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

import android.app.Activity
import android.app.Application
import android.os.Bundle
import androidx.appcompat.app.AlertDialog
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.Lifecycle
import anki.scheduler.CardAnswer.Rating
import com.google.android.material.snackbar.Snackbar
import com.ichi2.anki.AnkiDroidApp
import com.ichi2.anki.CollectionManager.withCol
import com.ichi2.anki.R
import com.ichi2.anki.common.coroutines.applicationScope
import com.ichi2.anki.libanki.CardId
import com.ichi2.anki.libanki.Collection
import com.ichi2.anki.libanki.Note
import com.ichi2.anki.libanki.NoteId
import com.ichi2.anki.snackbar.showSnackbar
import com.ichi2.anki.utils.ext.showDialogFragment
import com.ichi2.utils.create
import com.ichi2.utils.listItems
import com.ichi2.utils.negativeButton
import com.ichi2.utils.title
import kotlinx.coroutines.launch
import timber.log.Timber

/**
 * Shared gating + offer logic for Synapse's reviewer interactions, called by both
 * the MVVM reviewer ([com.ichi2.anki.ui.windows.reviewer.ReviewerViewModel]) and
 * the legacy reviewer ([com.ichi2.anki.AbstractFlashcardViewer]).
 *
 * Ports the desktop wiring in `qt/aqt/synapse/__init__.py::_on_answer_ai_offers`
 * plus `mint.py`, `generate.py`, and `tutor.py`. Everything here is strictly
 * ADDITIVE and self-gating: a miss on a non-MCAT card, or an unconfigured AI
 * service, is a silent no-op, so the base study loop is never affected.
 *
 * The reviewers only ever call [onAgainMiss] (a single line each). All work
 * happens off the UI thread via `withCol` / OkHttp, and any UI is shown on the
 * current foreground [FragmentActivity], resolved by a lightweight lifecycle
 * tracker (mirroring [com.ichi2.anki.ui.dialogs.ActivityAgnosticDialogs]) so the
 * shared logic never needs an Activity handed to it.
 */
object SynapseReviewerHooks {
    // --- Foreground-activity tracking ----------------------------------------

    private val startedActivities = mutableListOf<Activity>()
    private var callbacksRegistered = false

    private val currentActivity: FragmentActivity?
        get() =
            startedActivities
                .filterIsInstance<FragmentActivity>()
                .lastOrNull { it.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED) }

    /** Register the (idempotent) lifecycle tracker the first time a hook fires. */
    private fun ensureTracking() {
        if (callbacksRegistered) return
        val app = AnkiDroidApp.instance
        app.registerActivityLifecycleCallbacks(
            object : Application.ActivityLifecycleCallbacks {
                override fun onActivityStarted(activity: Activity) {
                    startedActivities.add(activity)
                }

                override fun onActivityStopped(activity: Activity) {
                    startedActivities.remove(activity)
                }

                override fun onActivityCreated(
                    activity: Activity,
                    savedInstanceState: Bundle?,
                ) {}

                override fun onActivityResumed(activity: Activity) {}

                override fun onActivityPaused(activity: Activity) {}

                override fun onActivitySaveInstanceState(
                    activity: Activity,
                    outState: Bundle,
                ) {}

                override fun onActivityDestroyed(activity: Activity) {
                    startedActivities.remove(activity)
                }
            },
        )
        callbacksRegistered = true
    }

    // --- Miss hook -----------------------------------------------------------

    /**
     * Post-answer Synapse hook. Called by both reviewers RIGHT AFTER the answer is
     * committed, with the id of the JUST-ANSWERED card (not the advanced-to card).
     *
     * A no-op unless [rating] is [Rating.AGAIN] and the answered note is an
     * MCAT-family item. Reads the note off the UI thread and, for a miss:
     *
     * * offers to mint a recall card when it's an "MCAT Application" (offline, B1);
     * * offers the state-grounded tutor when the AI service is configured (C2).
     *
     * Generation is offered at MASTERY, not at a miss (see desktop
     * `offer_generate_at_mastery`), so it is intentionally not triggered here.
     *
     * Never throws to the caller: all failures are logged and swallowed so the
     * reviewer is never broken by a Synapse side effect.
     */
    fun onAgainMiss(cardId: CardId) {
        if (cardId == 0L) return
        ensureTracking()
        applicationScope.launch {
            try {
                val info = withCol { missInfoForCard(this, cardId) } ?: return@launch
                showOffers(info)
            } catch (ex: Exception) {
                Timber.w(ex, "Synapse: miss hook failed")
            }
        }
    }

    /** Snapshot of the answered note needed to decide + show offers, read on the col thread. */
    private data class MissInfo(
        val noteId: NoteId,
        val isApplication: Boolean,
        val hasConcept: Boolean,
        val aiConfigured: Boolean,
    )

    private fun missInfoForCard(
        col: Collection,
        cardId: CardId,
    ): MissInfo? {
        val note = col.getCard(cardId).note(col)
        val name = note.notetype.name
        if (!Synapse.isMcatFamilyNotetype(name)) return null
        return MissInfo(
            noteId = note.id,
            isApplication = Synapse.isApplicationNotetype(name),
            hasConcept = Synapse.conceptTagsOf(note.tags).isNotEmpty(),
            aiConfigured = (col.config.get<String>(Synapse.CONFIG_SERVICE_URL) ?: "").isNotBlank(),
        )
    }

    private fun showOffers(info: MissInfo) {
        val activity = currentActivity ?: return
        // Both the offline mint (B1) and the state-grounded tutor (C2) apply to an
        // MCAT Application miss when the AI service is configured and the note has a
        // concept to ground on. Desktop surfaces both affordances independently
        // (mint.py + tutor.py); a snackbar has only one action, so offer a chooser
        // rather than dropping the tutor.
        val tutorAvailable = info.aiConfigured && info.hasConcept

        if (info.isApplication) {
            if (tutorAvailable) {
                activity.showSnackbar(R.string.synapse_offer_mint_or_tutor, Snackbar.LENGTH_LONG) {
                    setAction(R.string.synapse_offer_options_action) {
                        showMintOrTutorChooser(info.noteId)
                    }
                }
            } else {
                // AI off: keep the flagship one-tap mint affordance.
                activity.showSnackbar(R.string.synapse_offer_mint, Snackbar.LENGTH_LONG) {
                    setAction(R.string.synapse_offer_mint_action) {
                        mintFromMiss(info.noteId)
                    }
                }
            }
            return
        }

        // Non-Application MCAT-family miss: tutor offer only.
        if (tutorAvailable) {
            activity.showSnackbar(R.string.synapse_offer_tutor, Snackbar.LENGTH_LONG) {
                setAction(R.string.synapse_offer_tutor_action) {
                    openTutorForNote(info.noteId)
                }
            }
        }
    }

    /** Chooser shown when an Application miss can both mint a recall card and open the tutor. */
    private fun showMintOrTutorChooser(noteId: NoteId) {
        val activity = currentActivity ?: return
        val labels =
            listOf(
                activity.getString(R.string.synapse_offer_mint_action),
                activity.getString(R.string.synapse_offer_tutor_action),
            )
        AlertDialog
            .Builder(activity)
            .create {
                title(R.string.synapse_offer_mint_or_tutor)
                listItems(labels) { _, index ->
                    when (index) {
                        0 -> mintFromMiss(noteId)
                        else -> openTutorForNote(noteId)
                    }
                }
                negativeButton(R.string.dialog_cancel)
            }.show()
    }

    // --- Minting -------------------------------------------------------------

    /** Mint a recall card from [sourceNoteId] and toast the result. */
    fun mintFromMiss(sourceNoteId: NoteId) {
        applicationScope.launch {
            try {
                Minting.mintFromNote(sourceNoteId)
                currentActivity?.showSnackbar(R.string.synapse_minted)
            } catch (ex: Exception) {
                Timber.w(ex, "Synapse: mint failed")
            }
        }
    }

    // --- Grounded generation -------------------------------------------------

    /**
     * Fetch a grounded draft for [conceptTag] and open the review dialog.
     *
     * [sourceNoteId] (a missed note) links the approved item back to its source
     * and supplies a concept-name hint. Degrades quietly when the service is off
     * or unreachable.
     */
    fun generateForConcept(
        conceptTag: String,
        sourceNoteId: NoteId?,
    ) {
        val tag = conceptTag.trim()
        if (tag.isEmpty()) return
        applicationScope.launch {
            if (!SynapseAiClient.isConfigured()) {
                currentActivity?.showSnackbar(R.string.synapse_ai_not_configured)
                return@launch
            }
            val conceptHint =
                if (sourceNoteId != null) {
                    withCol {
                        val note = getNote(sourceNoteId)
                        if (Synapse.FIELD_CONCEPT in note) note.getItem(Synapse.FIELD_CONCEPT).trim() else ""
                    }
                } else {
                    ""
                }
            val response =
                try {
                    SynapseAiClient.generate(tag)
                } catch (ex: SynapseAiClient.ServiceUnavailable) {
                    Timber.w(ex, "Synapse: generate failed")
                    currentActivity?.showSnackbar(R.string.synapse_ai_unavailable)
                    return@launch
                }
            if (!response.isDraft) {
                val message = response.message ?: response.reason
                currentActivity?.showSnackbar(message ?: getStringSafe(R.string.synapse_generate_no_draft))
                return@launch
            }
            val activity = currentActivity ?: return@launch
            activity.showDialogFragment(
                GenerateReviewDialog.newInstance(
                    response = response,
                    conceptTag = tag,
                    conceptHint = conceptHint,
                    sourceNoteId = sourceNoteId,
                ),
            )
        }
    }

    // --- State-grounded tutor ------------------------------------------------

    /**
     * Assemble the state bundle for [noteId] and open the tutor panel.
     *
     * Assembles the bundle from local read-models (the `conceptMastery` RPC + the
     * note's explanation) then POSTs it to the service. Refuses locally (quiet
     * toast) when there is nothing to ground on. Degrades quietly on any failure.
     */
    fun openTutorForNote(noteId: NoteId) {
        applicationScope.launch {
            if (!SynapseAiClient.isConfigured()) {
                currentActivity?.showSnackbar(R.string.synapse_ai_not_configured)
                return@launch
            }
            val request = withCol { assembleTutorRequest(this, noteId) }
            if (request == null) {
                currentActivity?.showSnackbar(R.string.synapse_tutor_nothing)
                return@launch
            }
            val response =
                try {
                    SynapseAiClient.tutor(request)
                } catch (ex: SynapseAiClient.ServiceUnavailable) {
                    Timber.w(ex, "Synapse: tutor failed")
                    currentActivity?.showSnackbar(R.string.synapse_ai_unavailable)
                    return@launch
                }
            val turns = response.assistantTexts()
            if (turns.isEmpty()) {
                currentActivity?.showSnackbar(R.string.synapse_tutor_no_guidance)
                return@launch
            }
            val activity = currentActivity ?: return@launch
            activity.showDialogFragment(
                TutorDialog.newInstance(turns, response.surfacedPrerequisite),
            )
        }
    }

    private const val MASTERY_SEARCH = "deck:\"${Synapse.DECK_NAME}\""

    /**
     * Collection-thread body: build the tutor request, or `null` when nothing to
     * ground on (no concept tag, or no explanation text). Mirrors
     * `qt/aqt/synapse/tutor.py::_assemble_bundle`.
     */
    private fun assembleTutorRequest(
        col: Collection,
        noteId: NoteId,
    ): TutorRequest? {
        val note = col.getNote(noteId)
        val concepts = Synapse.conceptTagsOf(note.tags)
        val explanation = explanationOf(note)
        if (concepts.isEmpty() || explanation.isEmpty()) return null

        // Call the core RPC; do NOT compute mastery by hand.
        val bundles = col.backend.conceptMastery(concepts = concepts, search = MASTERY_SEARCH)
        val primary = bundles.firstOrNull()

        return TutorRequest(
            concept = concepts.first(),
            itemExplanation = explanation,
            answer = if (Synapse.FIELD_ANSWER in note) note.getItem(Synapse.FIELD_ANSWER).trim() else "",
            masteryBundle = bundleToJson(primary),
        )
    }

    /** The item's verified explanation (Explanation field, else ModelAnswer). */
    private fun explanationOf(note: Note): String {
        for (field in listOf(Synapse.FIELD_EXPLANATION, Synapse.FIELD_MODEL_ANSWER)) {
            if (field in note) {
                val text = note.getItem(field).trim()
                if (text.isNotEmpty()) return text
            }
        }
        return ""
    }

    private fun bundleToJson(bundle: anki.stats.ConceptMasteryBundleResponse.Bundle?): MasteryBundle {
        if (bundle == null) return MasteryBundle()
        val focus = if (bundle.hasFocus()) stateToJson(bundle.focus) else null
        return MasteryBundle(
            focus = focus,
            prerequisites = bundle.prerequisitesList.map { stateToJson(it) },
        )
    }

    private fun stateToJson(state: anki.stats.ConceptMasteryBundleResponse.ConceptState): ConceptState =
        ConceptState(
            concept = state.concept,
            section = state.section,
            memory = state.memory,
            cardCount = state.cardCount,
            scoredCardCount = state.scoredCardCount,
            sufficientData = state.sufficientData,
            mastered = state.mastered,
            hasCards = state.hasCards,
        )

    private fun getStringSafe(resId: Int): String = currentActivity?.getString(resId) ?: ""
}
