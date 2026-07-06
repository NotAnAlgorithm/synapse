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

import androidx.appcompat.app.AlertDialog
import androidx.fragment.app.FragmentActivity
import com.ichi2.anki.CollectionManager.withCol
import com.ichi2.anki.R
import com.ichi2.anki.launchCatchingTask
import com.ichi2.anki.snackbar.showSnackbar
import com.ichi2.utils.create
import com.ichi2.utils.listItems
import com.ichi2.utils.negativeButton
import com.ichi2.utils.title

/**
 * Menu-driven entry to Synapse's grounded generation (the Android analog of the
 * desktop "Synapse: Generate practice..." Tools action in
 * `qt/aqt/synapse/generate.py::pick_concept_and_generate`).
 *
 * Lists the collection's leaf `concept::<section>::<id>` tags, lets the user pick
 * one, and hands off to [SynapseReviewerHooks.generateForConcept] which fetches a
 * grounded draft and opens the review dialog. The whole flow degrades quietly when
 * the AI service is not configured.
 */
object SynapseGenerate {
    fun pickConceptAndGenerate(activity: FragmentActivity) {
        activity.launchCatchingTask {
            // Leaf concept tags only (`concept::section::id`), sorted + de-duplicated.
            val tags =
                withCol { tags.all() }
                    .filter { it.startsWith(Synapse.CONCEPT_TAG_PREFIX) && it.split("::").size >= 3 }
                    .distinct()
                    .sorted()
            if (tags.isEmpty()) {
                activity.showSnackbar(R.string.synapse_generate_no_concepts)
                return@launchCatchingTask
            }
            val labels = tags.map { prettify(it) }
            AlertDialog
                .Builder(activity)
                .create {
                    title(R.string.synapse_generate_pick_concept)
                    listItems(labels) { _, index ->
                        SynapseReviewerHooks.generateForConcept(activity, tags[index], sourceNoteId = null)
                    }
                    negativeButton(R.string.dialog_cancel)
                }.show()
        }
    }

    /** `concept::biochem::amino_acid_charge` -> `biochem › amino acid charge`. */
    private fun prettify(tag: String): String =
        tag
            .removePrefix(Synapse.CONCEPT_TAG_PREFIX)
            .split("::")
            .joinToString(" › ") { it.replace('_', ' ') }
}
