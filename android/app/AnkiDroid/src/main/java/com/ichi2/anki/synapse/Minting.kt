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

import anki.collection.OpChangesWithCount
import com.ichi2.anki.libanki.Collection
import com.ichi2.anki.libanki.Note
import com.ichi2.anki.libanki.NoteId
import com.ichi2.anki.observability.undoableOp
import timber.log.Timber

/**
 * Error-driven card minting (PRD B1), ported from `qt/aqt/synapse/mint.py`.
 *
 * The flagship Synapse interaction: when a student answers an "MCAT Application"
 * card *Again*, we treat that as a knowledge gap and let them mint a focused
 * recall card from it, linked back to the source note.
 *
 * The minted card is a stock "Basic" note (Front = source Stem [+ Concept as a
 * hint], Back = Answer + Explanation), carrying only the source's `concept::`
 * tags, added to the "Synapse" deck; the generated card's `custom_data` is
 * stamped with `{"src": <source note id>}` for lineage.
 */
object Minting {
    private const val BASIC_NOTETYPE_NAME = "Basic"
    private const val BASIC_FRONT = "Front"
    private const val BASIC_BACK = "Back"

    /**
     * Build + add a recall note from [sourceNoteId] and stamp its card's lineage.
     *
     * Wrapped in [undoableOp] so it notifies observers (and is undoable). Runs the
     * whole build on the collection thread via `withCol` inside [undoableOp].
     * Returns the [OpChanges] from adding the note.
     *
     * Safe to call more than once: each call mints a fresh recall card (matching
     * desktop, which offers the affordance per miss).
     */
    suspend fun mintFromNote(sourceNoteId: NoteId): OpChangesWithCount =
        undoableOp {
            mint(this, sourceNoteId)
        }

    private fun mint(
        col: Collection,
        sourceNoteId: NoteId,
    ): OpChangesWithCount {
        val source = col.getNote(sourceNoteId)

        val basic =
            col.notetypes.byName(BASIC_NOTETYPE_NAME)
                ?: throw IllegalStateException("notetype \"$BASIC_NOTETYPE_NAME\" not found")

        val note = col.newNote(basic)
        if (BASIC_FRONT in note) note.setItem(BASIC_FRONT, frontPrompt(source))
        if (BASIC_BACK in note) note.setItem(BASIC_BACK, backAnswer(source))
        // Copy only the source's concept tags so the minted card stays in the
        // same concept lineage for the memory graph.
        note.tags = Synapse.conceptTagsOf(source.tags).toMutableList()

        val deckId = col.decks.id(Synapse.DECK_NAME)
        val changes = col.addNote(note, deckId)

        // Stamp lineage on the generated card. Basic yields exactly one card.
        val customData = """{"src":${source.id}}"""
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

        Timber.i("Synapse: minted a recall card from note %d", sourceNoteId)
        return changes
    }

    /** A focused recall prompt derived from the source note (Stem, else Concept). */
    private fun frontPrompt(source: Note): String {
        val stem = if (Synapse.FIELD_STEM in source) source.getItem(Synapse.FIELD_STEM).trim() else ""
        val concept = if (Synapse.FIELD_CONCEPT in source) source.getItem(Synapse.FIELD_CONCEPT).trim() else ""
        if (stem.isNotEmpty()) {
            return if (concept.isNotEmpty()) "$stem<br><br><i>Concept: $concept</i>" else stem
        }
        return if (concept.isNotEmpty()) "Recall: $concept" else "Recall"
    }

    /** The answer + explanation from the source note. */
    private fun backAnswer(source: Note): String {
        val answer = if (Synapse.FIELD_ANSWER in source) source.getItem(Synapse.FIELD_ANSWER).trim() else ""
        val explanation =
            if (Synapse.FIELD_EXPLANATION in source) source.getItem(Synapse.FIELD_EXPLANATION).trim() else ""
        return listOf(answer, explanation).filter { it.isNotEmpty() }.joinToString("<br><br>")
    }
}
