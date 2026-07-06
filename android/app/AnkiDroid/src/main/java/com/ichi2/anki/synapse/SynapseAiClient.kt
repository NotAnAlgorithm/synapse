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

import com.ichi2.anki.CollectionManager.withCol
import com.ichi2.anki.web.HttpFetcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import timber.log.Timber
import java.util.concurrent.TimeUnit

/**
 * Client-side access to the Synapse AI service (Supabase Edge Functions), mirroring
 * `qt/aqt/synapse/service_client.py`.
 *
 * The Rust core makes NO network calls; this client shell reaches the hosted
 * service over HTTPS and commits results through the normal core RPCs (addNote,
 * ...). The service is OPTIONAL: when [CONFIG_SERVICE_URL][Synapse.CONFIG_SERVICE_URL]
 * is empty, ALL AI features are disabled and the base study loop is unaffected.
 *
 * Structured service refusals/errors (JSON bodies with a `status`, even on a 4xx/5xx
 * like 422 refused or 503 generator_unavailable) are returned to the caller so it can
 * surface the specific message. Only hard failures (transport error, non-JSON error
 * body, empty body) become [ServiceUnavailable]; either way the reviewer never crashes.
 */
object SynapseAiClient {
    /** Generation can call an LLM, so allow a generous timeout (mirrors desktop's 90s). */
    private const val TIMEOUT_SECS = 90L

    private val jsonMediaType = "application/json".toMediaType()

    /** Lenient parser: the service body shape is loosely typed and may add fields. */
    private val json =
        Json {
            ignoreUnknownKeys = true
            coerceInputValues = true
            isLenient = true
        }

    /** Reuse the app's OkHttp builder but with the longer AI timeouts. */
    private val client: OkHttpClient by lazy {
        HttpFetcher
            .getOkHttpBuilder(false)
            .connectTimeout(TIMEOUT_SECS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECS, TimeUnit.SECONDS)
            .writeTimeout(TIMEOUT_SECS, TimeUnit.SECONDS)
            .build()
    }

    /** Any failure reaching or using the Synapse AI service. */
    class ServiceUnavailable(
        message: String,
        cause: Throwable? = null,
    ) : Exception(message, cause)

    // --- Configuration -------------------------------------------------------

    /** The configured Edge-Functions base URL (trailing slash trimmed), or "". */
    suspend fun serviceUrl(): String = configString(Synapse.CONFIG_SERVICE_URL).trim().trimEnd('/')

    /** True once a service URL is set; gate AI affordances on this. */
    suspend fun isConfigured(): Boolean = serviceUrl().isNotEmpty()

    private suspend fun configString(key: String): String = withCol { config.get<String>(key) } ?: ""

    // --- Endpoints -----------------------------------------------------------

    /**
     * Request a grounded DRAFT item for a concept (never auto-approved).
     *
     * @throws ServiceUnavailable when the service is off or the call fails.
     */
    suspend fun generate(
        conceptTag: String,
        instruction: String = "",
    ): GenerateResponse {
        val body =
            buildJsonObject {
                put("concept_tag", conceptTag)
                put("instruction", instruction)
            }
        val raw = post("generate", body)
        return try {
            json.decodeFromString<GenerateResponse>(raw)
        } catch (ex: SerializationException) {
            throw ServiceUnavailable("generate returned an unexpected response", ex)
        }
    }

    /**
     * Send a student-state bundle to the tutor endpoint; return its turn(s).
     *
     * @throws ServiceUnavailable when the service is off or the call fails.
     */
    suspend fun tutor(request: TutorRequest): TutorResponse {
        val body = json.encodeToJsonElement(TutorRequest.serializer(), request).jsonObject
        val raw = post("tutor", body)
        return try {
            json.decodeFromString<TutorResponse>(raw)
        } catch (ex: SerializationException) {
            throw ServiceUnavailable("tutor returned an unexpected response", ex)
        }
    }

    // --- Transport -----------------------------------------------------------

    /**
     * POST a JSON body to one Edge Function and return the decoded text body.
     *
     * Runs blocking I/O on [Dispatchers.IO]. Any transport error, HTTP >= 400, or
     * empty body becomes a [ServiceUnavailable] so the study loop is never blocked.
     */
    private suspend fun post(
        function: String,
        body: JsonObject,
    ): String {
        val base = serviceUrl()
        if (base.isEmpty()) {
            throw ServiceUnavailable("The Synapse AI service is not configured.")
        }
        val key = configString(Synapse.CONFIG_SERVICE_KEY)
        val token = configString(Synapse.CONFIG_SERVICE_TOKEN).ifEmpty { key }

        val requestBuilder =
            Request
                .Builder()
                .url("$base/$function")
                .post(body.toString().toRequestBody(jsonMediaType))
                .header("Content-Type", "application/json")
        if (token.isNotEmpty()) {
            requestBuilder.header("Authorization", "Bearer $token")
        }
        if (key.isNotEmpty()) {
            // Supabase's gateway wants the anon/publishable key as `apikey`.
            requestBuilder.header("apikey", key)
        }
        val request = requestBuilder.build()

        return withContext(Dispatchers.IO) {
            try {
                client.newCall(request).execute().use { response ->
                    val text = response.body.string()
                    if (!response.isSuccessful) {
                        Timber.w("Synapse: %s -> HTTP %d: %s", function, response.code, text.take(500))
                        // The service returns structured refusals/errors as JSON with a
                        // 4xx/5xx status (e.g. 422 refused/rejected, 503
                        // generator_unavailable). Pass those through so the caller can
                        // surface the specific `status`/`message`; only a non-JSON error
                        // body (gateway/auth/HTML) is treated as a hard failure.
                        if (!text.trimStart().startsWith("{")) {
                            throw ServiceUnavailable("$function failed (${response.code}): ${text.take(300)}")
                        }
                    } else if (text.isBlank()) {
                        throw ServiceUnavailable("$function returned an empty response")
                    }
                    text
                }
            } catch (ex: ServiceUnavailable) {
                throw ex
            } catch (ex: Exception) {
                Timber.w(ex, "Synapse: could not reach the AI service")
                throw ServiceUnavailable("could not reach the Synapse service: ${ex.message}", ex)
            }
        }
    }
}

// --- Response / request models -----------------------------------------------

/**
 * The `generate` response. Only a `status` in {"", "draft"} carrying a dict `item`
 * is a real draft; any other status surfaces [message]. Mirrors desktop's
 * `_normalise_draft`.
 */
@Serializable
data class GenerateResponse(
    val status: String = "",
    val item: GenerateItem? = null,
    val citation: Citation? = null,
    val message: String? = null,
    val reason: String? = null,
) {
    /** True when this is a well-formed draft the reviewer can approve/edit/reject. */
    val isDraft: Boolean get() = (status == "" || status == "draft") && item != null
}

@Serializable
data class GenerateItem(
    val stem: String = "",
    val options: List<String> = emptyList(),
    val answerIndex: Int? = null,
    val explanation: String = "",
    val answer: String? = null,
) {
    /** Resolve the correct answer: `options[answerIndex]`, falling back to `answer`. */
    fun resolvedAnswer(): String {
        val idx = answerIndex
        if (idx != null && idx in options.indices) {
            val candidate = options[idx].trim()
            if (candidate.isNotEmpty()) return candidate
        }
        return answer?.trim().orEmpty()
    }
}

@Serializable
data class Citation(
    @kotlinx.serialization.SerialName("chunk_id") val chunkId: String = "",
    val title: String = "",
    val section: String = "",
    val anchor: String = "",
    val license: String = "",
) {
    /** Format as "{title} — {section} ({anchor}) [{license}]" (desktop parity). */
    fun formatted(): String {
        val mainParts = listOf(title.trim(), section.trim()).filter { it.isNotEmpty() }
        var text = if (mainParts.isNotEmpty()) mainParts.joinToString(" — ") else chunkId.trim()
        val a = anchor.trim()
        if (a.isNotEmpty()) text = if (text.isNotEmpty()) "$text ($a)" else a
        val l = license.trim()
        if (l.isNotEmpty()) text = if (text.isNotEmpty()) "$text [$l]" else l
        return text
    }
}

/** The tutor request bundle. See CLAUDE.md "tutor request body". */
@Serializable
data class TutorRequest(
    val concept: String,
    @kotlinx.serialization.SerialName("item_explanation") val itemExplanation: String,
    val answer: String,
    @kotlinx.serialization.SerialName("mastery_bundle") val masteryBundle: MasteryBundle,
)

@Serializable
data class MasteryBundle(
    val focus: ConceptState? = null,
    val prerequisites: List<ConceptState> = emptyList(),
)

/** Mirrors the proto `ConceptState`, projected to the service's JSON shape. */
@Serializable
data class ConceptState(
    val concept: String = "",
    val section: String = "",
    val memory: Float = 0f,
    @kotlinx.serialization.SerialName("card_count") val cardCount: Int = 0,
    @kotlinx.serialization.SerialName("scored_card_count") val scoredCardCount: Int = 0,
    @kotlinx.serialization.SerialName("sufficient_data") val sufficientData: Boolean = false,
    val mastered: Boolean = false,
    @kotlinx.serialization.SerialName("has_cards") val hasCards: Boolean = false,
)

/** The tutor response. Accepts the primary `turns` shape and a `reply` fallback. */
@Serializable
data class TutorResponse(
    val turns: List<TutorTurn> = emptyList(),
    @kotlinx.serialization.SerialName("surfaced_prerequisite") val surfacedPrerequisite: String? = null,
    @kotlinx.serialization.SerialName("giveaway_blocked") val giveawayBlocked: Boolean = false,
    val reply: String? = null,
    val status: String? = null,
    val message: String? = null,
    val reason: String? = null,
) {
    /** Assistant turn text(s), from `turns[].content` or the `reply` fallback. */
    fun assistantTexts(): List<String> {
        val texts = turns.mapNotNull { it.content?.trim()?.ifEmpty { null } }
        if (texts.isNotEmpty()) return texts
        val r = reply?.trim()
        return if (!r.isNullOrEmpty()) listOf(r) else emptyList()
    }
}

@Serializable
data class TutorTurn(
    val role: String = "",
    val content: String? = null,
)
