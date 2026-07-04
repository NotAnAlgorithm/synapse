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
import androidx.core.os.bundleOf
import androidx.core.view.isVisible
import androidx.fragment.app.DialogFragment
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.ichi2.anki.R
import com.ichi2.anki.databinding.SynapseDialogTutorBinding
import com.ichi2.utils.create
import com.ichi2.utils.positiveButton
import com.ichi2.utils.title

/**
 * A read-only Socratic tutor panel (ported from `qt/aqt/synapse/tutor.py`).
 *
 * Shows the returned assistant turn(s) and, when present, a muted
 * "Likely gap: {surfaced_prerequisite}" line. It NEVER reveals the answer — the
 * turns come from the state-grounded service, which is grounded in the item's
 * explanation, not its answer.
 */
class TutorDialog : DialogFragment() {
    override fun onCreateDialog(savedInstanceState: Bundle?): Dialog {
        val binding = SynapseDialogTutorBinding.inflate(layoutInflater)
        val args = requireArguments()

        val gap = args.getString(ARG_GAP)
        if (gap.isNullOrEmpty()) {
            binding.synapseTutorGap.isVisible = false
        } else {
            binding.synapseTutorGap.isVisible = true
            binding.synapseTutorGap.text = getString(R.string.synapse_tutor_gap, gap)
        }

        binding.synapseTutorBody.text = args.getStringArrayList(ARG_TURNS)?.joinToString("\n\n").orEmpty()

        return MaterialAlertDialogBuilder(requireContext()).create {
            title(R.string.synapse_tutor_title)
            setView(binding.root)
            positiveButton(R.string.synapse_close)
        }
    }

    companion object {
        private const val ARG_TURNS = "turns"
        private const val ARG_GAP = "gap"

        /**
         * @param turns assistant turn text(s) already extracted from the response
         * @param surfacedPrerequisite the weak prerequisite to surface, if any (human label)
         */
        fun newInstance(
            turns: List<String>,
            surfacedPrerequisite: String?,
        ): TutorDialog =
            TutorDialog().apply {
                arguments =
                    bundleOf(
                        ARG_TURNS to ArrayList(turns),
                        ARG_GAP to surfacedPrerequisite?.let { GenerateReviewDialog.conceptNameFromTag(it) },
                    )
            }
    }
}
