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
 * this program.  If not, see <http://www.gnu.org/licenses/>.
 */
package com.ichi2.anki.preferences

import androidx.preference.Preference
import com.ichi2.anki.CollectionManager.withCol
import com.ichi2.anki.R
import com.ichi2.anki.launchCatchingTask
import com.ichi2.anki.snackbar.showSnackbar
import com.ichi2.anki.synapse.Synapse
import com.ichi2.preferences.VersatileTextPreference
import timber.log.Timber
import java.time.LocalDate
import java.time.format.DateTimeParseException

/**
 * Native "Synapse" settings screen, mirroring the desktop
 * `qt/aqt/synapse/settings.py` dialog + the exam-date governor from
 * `qt/aqt/synapse/__init__.py::_set_exam_date`.
 *
 * The three AI-service values and the governor keys all live in the *synced*
 * collection config (namespaced `synapse:`, see [Synapse]) so they roam with the
 * desktop client — they are NOT stored in SharedPreferences. Each preference
 * therefore has [Preference.isPersistent] disabled: we load the current value
 * from the collection in [initSubscreen] and write it back through
 * `withCol { config.set(...) }` in the change listeners.
 *
 * The cloud-sync URL is Anki's *native* custom sync URL (identity = a
 * self-hosted Anki sync-server account), not a `synapse:` key, so this screen
 * cross-links to the existing [CustomSyncServerSettingsFragment] rather than
 * duplicating that setting (see notes/SYNC_SETUP.md).
 */
class SynapseSettingsFragment : SettingsFragment() {
    override val preferenceResource: Int
        get() = R.xml.preferences_synapse
    override val analyticsScreenNameConstant: String
        get() = "prefs.synapse"

    override fun initSubscreen() {
        setupServiceUrlPref()
        setupSecretPref(R.string.synapse_service_key_key, Synapse.CONFIG_SERVICE_KEY)
        setupSecretPref(R.string.synapse_service_token_key, Synapse.CONFIG_SERVICE_TOKEN)
        setupExamDatePref()
    }

    /**
     * Service URL: stored verbatim; an empty URL is the valid "AI off" state
     * (matches [Synapse]/the desktop `service_client` semantics), so it is not
     * validated as a URL here. Uses the XML `useSimpleSummaryProvider`, which
     * shows the current [VersatileTextPreference.getText] as the summary.
     */
    private fun setupServiceUrlPref() {
        requirePreference<VersatileTextPreference>(R.string.synapse_service_url_key).apply {
            isPersistent = false
            launchCatchingTask { text = getConfigString(Synapse.CONFIG_SERVICE_URL) }
            setOnPreferenceChangeListener { _, newValue ->
                val value = (newValue as? String)?.trim().orEmpty()
                text = value
                launchCatchingTask { setConfigString(Synapse.CONFIG_SERVICE_URL, value) }
                false // value applied manually above; nothing to persist to prefs
            }
        }
    }

    /**
     * A secret (anon key / service token): password input + we never echo the
     * value. The summary only reveals whether a value is currently set.
     */
    private fun setupSecretPref(
        keyResId: Int,
        configKey: String,
    ) {
        requirePreference<VersatileTextPreference>(keyResId).apply {
            isPersistent = false
            launchCatchingTask {
                text = getConfigString(configKey)
                updateSecretSummary(this@apply)
            }
            setOnPreferenceChangeListener { _, newValue ->
                val value = (newValue as? String)?.trim().orEmpty()
                text = value
                updateSecretSummary(this)
                launchCatchingTask { setConfigString(configKey, value) }
                false
            }
        }
    }

    private fun updateSecretSummary(preference: VersatileTextPreference) {
        preference.summary =
            if (preference.text.isNullOrEmpty()) {
                getString(R.string.synapse_value_not_set_summary)
            } else {
                getString(R.string.synapse_secret_set_summary)
            }
    }

    /**
     * Exam date (governor). Entering a valid `YYYY-MM-DD` sets
     * `synapse:test_date` and enables the governor; clearing the field disables
     * it. Mirrors the desktop `_set_exam_date`.
     */
    private fun setupExamDatePref() {
        requirePreference<VersatileTextPreference>(R.string.synapse_test_date_key).apply {
            isPersistent = false
            // Reject a non-empty, non-ISO date before the dialog can be accepted.
            continuousValidator =
                VersatileTextPreference.Validator { value ->
                    if (value.isNotEmpty()) LocalDate.parse(value)
                }
            launchCatchingTask {
                text = getConfigString(Synapse.CONFIG_TEST_DATE)
                updateExamDateSummary(this@apply)
            }
            setOnPreferenceChangeListener { _, newValue ->
                val value = (newValue as? String)?.trim().orEmpty()
                if (!isValidExamDate(value)) {
                    showSnackbar(R.string.synapse_test_date_invalid)
                    return@setOnPreferenceChangeListener false
                }
                text = value
                updateExamDateSummary(this)
                applyExamDate(value)
                false
            }
        }
    }

    private fun updateExamDateSummary(preference: VersatileTextPreference) {
        val date = preference.text
        preference.summary =
            if (date.isNullOrEmpty()) {
                getString(R.string.synapse_test_date_summary_off)
            } else {
                getString(R.string.synapse_test_date_summary_on, date)
            }
    }

    /**
     * Persist the governor state. A blank date turns the governor off; a set
     * date enables it and records the ISO date. Removing the stale
     * `synapse:test_date` on clear keeps the config tidy for the scheduler.
     */
    private fun applyExamDate(date: String) {
        val enabled = date.isNotEmpty()
        launchCatchingTask {
            withCol {
                config.set(Synapse.CONFIG_GOVERNOR_ENABLED, enabled)
                if (enabled) {
                    config.set(Synapse.CONFIG_TEST_DATE, date)
                } else {
                    config.remove(Synapse.CONFIG_TEST_DATE)
                }
            }
            Timber.i("Synapse governor %s", if (enabled) "enabled for $date" else "disabled")
        }
    }

    private fun isValidExamDate(date: String): Boolean {
        if (date.isEmpty()) return true
        return try {
            LocalDate.parse(date)
            true
        } catch (_: DateTimeParseException) {
            false
        }
    }

    private suspend fun getConfigString(configKey: String): String = withCol { config.get<String>(configKey) } ?: ""

    private suspend fun setConfigString(
        configKey: String,
        value: String,
    ) {
        withCol { config.set(configKey, value) }
        Timber.i("Synapse: saved config key '%s'", configKey)
    }
}
