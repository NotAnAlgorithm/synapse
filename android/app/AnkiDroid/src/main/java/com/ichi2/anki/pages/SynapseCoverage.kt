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
package com.ichi2.anki.pages

import android.content.Context
import android.os.Bundle
import android.view.View
import com.google.android.material.appbar.MaterialToolbar
import com.ichi2.anki.R
import com.ichi2.anki.SingleFragmentActivity

/**
 * Synapse AAMC coverage checker: which outline topics have a card, gap list and
 * per-section breakdown. Reuses the shared SvelteKit page at `ts/routes/coverage/`,
 * backed by the `conceptCoverage` read-model RPC.
 */
class SynapseCoverage : PageFragment() {
    override val pagePath = "coverage"

    override fun onViewCreated(
        view: View,
        savedInstanceState: Bundle?,
    ) {
        super.onViewCreated(view, savedInstanceState)
        view.findViewById<MaterialToolbar>(R.id.toolbar)?.setTitle(R.string.synapse_coverage_title)
    }

    companion object {
        fun getIntent(context: Context) = SingleFragmentActivity.getIntent(context, SynapseCoverage::class)
    }
}
