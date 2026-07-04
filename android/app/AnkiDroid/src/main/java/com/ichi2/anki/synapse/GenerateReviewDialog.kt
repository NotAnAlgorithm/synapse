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

import android.app.Dialog
import android.os.Bundle
import android.widget.EditText
import androidx.core.os.bundleOf
import androidx.core.view.isVisible
import androidx.fragment.app.DialogFragment
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.ichi2.anki.R
import com.ichi2.anki.databinding.SynapseDialogGenerateReviewBinding
import com.ichi2.anki.launchCatchingTask
import com.ichi2.anki.libanki.Collection
import com.ichi2.anki.libanki.NoteId
import com.ichi2.anki.observability.undoableOp
import com.ichi2.anki.snackbar.showSnackbar
import com.ichi2.utils.create
import com.ichi2.utils.negativeButton
import com.ichi2.utils.positiveButton
import com.ichi2.utils.title
import timber.log.Timber

/** The trimmed text of an edit field, or "" when empty. */
private fun EditText.trimmedText(): String = text?.toString()?.trim().orEmpty()

/**
 * Human review of a grounded [GenerateResponse] draft: Approve / Reject
 * (ported from `qt/aqt/synapse/generate.py::GenerateReviewDialog`).
 *
 * The drafted stem, answer, explanation and concept are shown in editable fields
 * (so "editing" is simply changing them before approving); the citation is shown
 * read-only and prominent — every approved item keeps its grounding (PRD C1). On
 * Approve the (possibly edited) draft is written as a real "MCAT Application"
 * note via [Collection.addNote] inside [undoableOp]; Reject/close discards it.
 */
class GenerateReviewDialog : DialogFragment() {
    override fun onCreateDialog(savedInstanceState: Bundle?): Dialog {
        val binding = SynapseDialogGenerateReviewBinding.inflate(layoutInflater)
        val args = requireArguments()

        binding.synapseEditConcept.setText(args.getString(ARG_CONCEPT).orEmpty())
        binding.synapseEditStem.setText(args.getString(ARG_STEM).orEmpty())
        binding.synapseEditAnswer.setText(args.getString(ARG_ANSWER).orEmpty())
        binding.synapseEditExplanation.setText(args.getString(ARG_EXPLANATION).orEmpty())

        val citation = args.getString(ARG_CITATION).orEmpty()
        binding.synapseCitation.text = citation
        binding.synapseCitationWarning.isVisible = citation.isEmpty()

        val conceptTag = args.getString(ARG_CONCEPT_TAG).orEmpty()
        val sourceNoteId = args.getLong(ARG_SOURCE_NOTE_ID, 0L).takeIf { it != 0L }

        return MaterialAlertDialogBuilder(requireContext()).create {
            title(R.string.synapse_generate_review_title)
            setView(binding.root)
            positiveButton(R.string.synapse_approve) {
                onApprove(
                    stem = binding.synapseEditStem.trimmedText(),
                    answer = binding.synapseEditAnswer.trimmedText(),
                    explanation = binding.synapseEditExplanation.trimmedText(),
                    concept = binding.synapseEditConcept.trimmedText(),
                    grounding = citation,
                    conceptTag = conceptTag,
                    sourceNoteId = sourceNoteId,
                )
            }
            negativeButton(R.string.synapse_reject)
        }
    }

    private fun onApprove(
        stem: String,
        answer: String,
        explanation: String,
        concept: String,
        grounding: String,
        conceptTag: String,
        sourceNoteId: NoteId?,
    ) {
        if (stem.isEmpty() || answer.isEmpty()) {
            showSnackbar(R.string.synapse_generate_needs_stem_answer)
            return
        }
        launchCatchingTask {
            undoableOp {
                addGeneratedNote(
                    col = this,
                    stem = stem,
                    answer = answer,
                    explanation = explanation,
                    concept = concept,
                    grounding = grounding,
                    conceptTag = conceptTag,
                    sourceNoteId = sourceNoteId,
                )
            }
            showSnackbar(R.string.synapse_generate_added)
        }
    }

    companion object {
        private const val ARG_CONCEPT = "concept"
        private const val ARG_STEM = "stem"
        private const val ARG_ANSWER = "answer"
        private const val ARG_EXPLANATION = "explanation"
        private const val ARG_CITATION = "citation"
        private const val ARG_CONCEPT_TAG = "conceptTag"
        private const val ARG_SOURCE_NOTE_ID = "sourceNoteId"

        /**
         * Build the review dialog from a service draft.
         *
         * @param conceptTag the concept the item was generated for (lineage fallback)
         * @param sourceNoteId when the draft came from a miss, for card `custom_data`
         */
        fun newInstance(
            response: GenerateResponse,
            conceptTag: String,
            conceptHint: String,
            sourceNoteId: NoteId?,
        ): GenerateReviewDialog {
            val item = response.item ?: GenerateItem()
            val concept = conceptHint.ifEmpty { conceptNameFromTag(conceptTag) }
            return GenerateReviewDialog().apply {
                arguments =
                    bundleOf(
                        ARG_CONCEPT to concept,
                        ARG_STEM to stemWithOptions(item.stem, item.options),
                        ARG_ANSWER to item.resolvedAnswer(),
                        ARG_EXPLANATION to item.explanation.trim(),
                        ARG_CITATION to (response.citation?.formatted().orEmpty()),
                        ARG_CONCEPT_TAG to conceptTag,
                        ARG_SOURCE_NOTE_ID to (sourceNoteId ?: 0L),
                    )
            }
        }

        private const val OPTION_LABELS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

        /** Compose the stem plus a lettered options block (MCAT has no Options field). */
        private fun stemWithOptions(
            stem: String,
            options: List<String>,
        ): String {
            val rendered =
                options
                    .mapIndexedNotNull { i, opt ->
                        val text = opt.trim()
                        if (text.isNotEmpty() && i < OPTION_LABELS.length) "${OPTION_LABELS[i]}. $text" else null
                    }
            val trimmedStem = stem.trim()
            if (rendered.isEmpty()) return trimmedStem
            val block = rendered.joinToString("\n")
            return if (trimmedStem.isNotEmpty()) "$trimmedStem\n\n$block" else block
        }

        /** Human-ish concept name from a `concept::section::id` tag's last segment. */
        fun conceptNameFromTag(conceptTag: String): String {
            val parts = conceptTag.split("::").filter { it.isNotEmpty() }
            return if (parts.isEmpty()) conceptTag else parts.last().replace("_", " ")
        }

        /**
         * Build + add an approved item as an "MCAT Application" note (collection thread).
         *
         * Fills the notetype's fields from the reviewed draft (including
         * [Grounding][Synapse.FIELD_GROUNDING] = citation), copies the source
         * concept tags, and — when a source note is known — stamps the generated
         * card's `custom_data` with the lineage, mirroring [Minting].
         */
        private fun addGeneratedNote(
            col: Collection,
            stem: String,
            answer: String,
            explanation: String,
            concept: String,
            grounding: String,
            conceptTag: String,
            sourceNoteId: NoteId?,
        ): anki.collection.OpChangesWithCount {
            val notetype =
                col.notetypes.byName(Synapse.MCAT_NOTETYPE_NAME)
                    ?: throw IllegalStateException("notetype \"${Synapse.MCAT_NOTETYPE_NAME}\" not found")

            val note = col.newNote(notetype)
            if (Synapse.FIELD_STEM in note) note.setItem(Synapse.FIELD_STEM, stem)
            if (Synapse.FIELD_ANSWER in note) note.setItem(Synapse.FIELD_ANSWER, answer)
            if (Synapse.FIELD_EXPLANATION in note) note.setItem(Synapse.FIELD_EXPLANATION, explanation)
            if (Synapse.FIELD_CONCEPT in note) note.setItem(Synapse.FIELD_CONCEPT, concept)
            if (Synapse.FIELD_GROUNDING in note) note.setItem(Synapse.FIELD_GROUNDING, grounding)

            val tags = lineageTags(col, conceptTag, sourceNoteId)
            if (tags.isNotEmpty()) note.tags = tags.toMutableList()

            val deckId = col.decks.id(Synapse.DECK_NAME)
            val changes = col.addNote(note, deckId)

            if (sourceNoteId != null) {
                val customData = """{"src":$sourceNoteId}"""
                val backendCards =
                    note.cardIds(col).map { cardId ->
                        col
                            .getCard(cardId)
                            .toBackendCard()
                            .toBuilder()
                            .setCustomData(customData)
                            .build()
                    }
                if (backendCards.isNotEmpty()) {
                    col.backend.updateCards(backendCards, skipUndoEntry = true)
                }
            }

            Timber.i("Synapse: added a generated MCAT Application note")
            return changes
        }

        /** Concept tags to attach to the generated note (source's tags, else the concept tag). */
        private fun lineageTags(
            col: Collection,
            conceptTag: String,
            sourceNoteId: NoteId?,
        ): List<String> {
            if (sourceNoteId != null) {
                val copied = Synapse.conceptTagsOf(col.getNote(sourceNoteId).tags)
                if (copied.isNotEmpty()) return copied
            }
            return if (conceptTag.startsWith(Synapse.CONCEPT_TAG_PREFIX)) listOf(conceptTag) else emptyList()
        }
    }
}
