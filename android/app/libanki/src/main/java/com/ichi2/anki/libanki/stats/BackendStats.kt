/*
 * Copyright (c) 2022 Ankitects Pty Ltd <http://apps.ankiweb.net>
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
 * this program.  If not, see <http://www.gnu.org/licenses/>.
 */

package com.ichi2.anki.libanki.stats

import com.ichi2.anki.libanki.Collection

// These take and return bytes that the frontend TypeScript code will encode/decode.
fun Collection.cardStatsRaw(input: ByteArray): ByteArray = backend.cardStatsRaw(input)

fun Collection.graphsRaw(input: ByteArray): ByteArray = backend.graphsRaw(input)

fun Collection.getGraphPreferencesRaw(): ByteArray {
    val prefs =
        backend
            .getGraphPreferences()
            .toBuilder()
            .setBrowserLinksSupported(false)
            .build()
    return prefs.toByteArray()
}

fun Collection.setGraphPreferencesRaw(input: ByteArray): ByteArray = backend.setGraphPreferencesRaw(input)

// Synapse read-model RPCs (StatsService). These back the Synapse dashboard, coverage
// and concept-graph SvelteKit pages, mirroring the desktop `exposed_backend_list`.
fun Collection.conceptMemoryRaw(input: ByteArray): ByteArray = backend.conceptMemoryRaw(input)

fun Collection.conceptCoverageRaw(input: ByteArray): ByteArray = backend.conceptCoverageRaw(input)

fun Collection.conceptGraphRaw(input: ByteArray): ByteArray = backend.conceptGraphRaw(input)

fun Collection.conceptPerformanceRaw(input: ByteArray): ByteArray = backend.conceptPerformanceRaw(input)

// Persists `synapse:streak` as a side effect, matching the desktop AdoptionStats RPC.
fun Collection.adoptionStatsRaw(input: ByteArray): ByteArray = backend.adoptionStatsRaw(input)

fun Collection.conceptMasteryRaw(input: ByteArray): ByteArray = backend.conceptMasteryRaw(input)

fun Collection.experimentMetricsRaw(input: ByteArray): ByteArray = backend.experimentMetricsRaw(input)
