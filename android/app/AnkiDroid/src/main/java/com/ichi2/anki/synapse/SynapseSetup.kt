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
import com.ichi2.anki.withProgress
import com.ichi2.utils.create
import com.ichi2.utils.message
import com.ichi2.utils.negativeButton
import com.ichi2.utils.positiveButton
import com.ichi2.utils.title
import timber.log.Timber

/**
 * UI entry point for Synapse provisioning (the "Synapse: Set up" nav-drawer
 * action).
 *
 * Shows a confirmation dialog and, on confirm, runs [Provisioner.provision] off
 * the main thread behind a progress indicator, then reports a summary snackbar.
 * Provisioning is idempotent, so re-running is safe.
 */
object SynapseSetup {
    /**
     * Prompt the user, then provision the Synapse study environment with the
     * recommended defaults (all scheduler features on, adoption on, seed content
     * on, governor off).
     */
    fun confirmAndProvision(activity: FragmentActivity) {
        AlertDialog
            .Builder(activity)
            .create {
                title(R.string.synapse_setup_dialog_title)
                message(R.string.synapse_setup_dialog_message)
                positiveButton(R.string.synapse_setup_confirm) {
                    provision(activity)
                }
                negativeButton(R.string.dialog_cancel)
            }.show()
    }

    /** Run provisioning off the main thread and report the result. */
    private fun provision(activity: FragmentActivity) {
        activity.launchCatchingTask {
            val result =
                activity.withProgress(activity.getString(R.string.synapse_setup_progress)) {
                    withCol { Provisioner.provision(this) }
                }
            Timber.i(
                "Synapse provisioning complete: %d notetypes, %d notes, deck=%d, fsrs=%b",
                result.notetypesCount,
                result.totalNotesAdded,
                result.deckId,
                result.fsrs,
            )
            activity.showSnackbar(
                activity.resources.getQuantityString(
                    R.plurals.synapse_setup_complete,
                    result.totalNotesAdded,
                    result.notetypesCount,
                    result.totalNotesAdded,
                ),
            )
        }
    }
}
