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

/**
 * Single source of truth for Synapse's cross-cutting names and conventions on
 * Android, mirroring the desktop `qt/aqt/synapse/` constants.
 *
 * The desktop core deliberately copy-pastes the `MCAT ` notetype-name prefix and
 * the `concept::` tag prefix across several Rust modules (with "keep in sync"
 * notes); this object keeps the Android side from adding yet more scattered
 * copies. Provisioning ([Provisioner]), the reviewer miss-hook / minting, the AI
 * service client and the settings screen all read from here.
 */
object Synapse {
    // --- Deck / preset (see qt/aqt/synapse/provision.py) ---------------------
    const val PRESET_NAME = "Synapse"
    const val DECK_NAME = "Synapse"

    /** Do NOT lower this; 0.9 is the intended default retention for the demo. */
    const val DESIRED_RETENTION = 0.9

    // --- Notetypes -----------------------------------------------------------
    const val MCAT_NOTETYPE_NAME = "MCAT Application"
    const val WHICH_PRINCIPLE_NOTETYPE_NAME = "MCAT Which-Principle"
    const val DATA_SNIPPET_NOTETYPE_NAME = "MCAT Data-Snippet"
    const val EXPLAIN_WHY_NOTETYPE_NAME = "MCAT Explain-Why"

    /**
     * Notetype-name prefix used by the core scheduler/stats to classify an
     * "application" item (as opposed to a plain recall card). Must match the
     * `MCAT ` prefix hard-coded in rslib.
     */
    const val MCAT_NOTETYPE_PREFIX = "MCAT "

    // --- MCAT Application fields (order is append-only: Grounding stays last) --
    const val FIELD_STEM = "Stem"
    const val FIELD_PASSAGE = "Passage"
    const val FIELD_ANSWER = "Answer"
    const val FIELD_EXPLANATION = "Explanation"
    const val FIELD_CONCEPT = "Concept"
    const val FIELD_GROUNDING = "Grounding"

    /** Model-answer field used by the self-graded "Explain-Why" item notetype. */
    const val FIELD_MODEL_ANSWER = "ModelAnswer"

    val MCAT_FIELDS =
        listOf(FIELD_STEM, FIELD_PASSAGE, FIELD_ANSWER, FIELD_EXPLANATION, FIELD_CONCEPT, FIELD_GROUNDING)

    // --- Concept tags (concept::<section>::<id>) -----------------------------
    const val CONCEPT_TAG_PREFIX = "concept::"

    /** The section is the 2nd `::` segment: `concept::biochem::x` -> `biochem`. */
    fun sectionOf(conceptTag: String): String? = conceptTag.split("::").getOrNull(1)

    /** Concept tags carried by a note (used for mint-time inheritance + grounding). */
    fun conceptTagsOf(tags: Collection<String>): List<String> = tags.filter { it.startsWith(CONCEPT_TAG_PREFIX) }

    // --- Generic collection-config keys (synced; shared with desktop) --------
    const val CONFIG_SERVICE_URL = "synapse:service_url"
    const val CONFIG_SERVICE_KEY = "synapse:service_key"
    const val CONFIG_SERVICE_TOKEN = "synapse:service_token"
    const val CONFIG_ADOPTION_ENABLED = "synapse:adoption_enabled"
    const val CONFIG_GOVERNOR_ENABLED = "synapse:governor_enabled"
    const val CONFIG_TEST_DATE = "synapse:test_date"

    // --- Notetype-name classification helpers --------------------------------

    /** The exact "MCAT Application" notetype (the mint source + generation target). */
    fun isApplicationNotetype(notetypeName: String): Boolean = notetypeName == MCAT_NOTETYPE_NAME

    /** Any MCAT-family notetype (Application + the three item notetypes). */
    fun isMcatFamilyNotetype(notetypeName: String): Boolean = notetypeName.startsWith(MCAT_NOTETYPE_PREFIX)
}
