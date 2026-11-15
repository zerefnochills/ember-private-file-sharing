package com.ember.mobile

import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL

data class PowChallenge(val challenge: String, val difficulty: UInt, val expiresAt: ULong)

data class CreatedBlob(
    val id: String,
    val deleteToken: String,
    val expiresAt: ULong,
    val maxPartBytes: Int,
)

data class BlobMetadata(
    val size: ULong,
    val parts: Int,
    val burn: Boolean,
    val expiresAt: ULong,
    val state: String,
)

data class BlobClaim(val token: String?, val expiresAt: ULong?)

/** HTTPS-only client for the v1 relay API. Invoke from an IO thread. */
class RelayClient internal constructor(
    baseUrl: String,
    private val connectionFactory: (URL) -> HttpURLConnection = { it.openConnection() as HttpURLConnection },
) {
    private val origin: String = validateOrigin(baseUrl)

    fun getPowChallenge(): PowChallenge {
        val json = jsonRequest("GET", "/v1/pow/challenge")
        return PowChallenge(
            challenge = json.requiredString("challenge"),
            difficulty = json.requiredLong("difficulty").also { require(it in 0..UInt.MAX_VALUE.toLong()) }.toUInt(),
            expiresAt = json.requiredLong("expires_at").toULong(),
        )
    }

    fun createBlob(
        size: ULong,
        parts: Int,
        burn: Boolean,
        ttlSeconds: Long,
        challenge: String,
        nonce: ULong,
    ): CreatedBlob {
        require(size in 1uL..Long.MAX_VALUE.toULong() && parts > 0 && ttlSeconds in 1..MAX_TTL_SECONDS)
        require(challenge.isNotBlank())
        val body = JSONObject()
            .put("size", size.toLong())
            .put("parts", parts)
            .put("burn", burn)
            .put("ttl_secs", ttlSeconds)
        val json = jsonRequest(
            method = "POST",
            path = "/v1/blobs",
            headers = mapOf(
                "X-Pow-Challenge" to challenge,
                "X-Pow-Nonce" to nonce.toString(),
            ),
            body = body.toString().toByteArray(Charsets.UTF_8),
        )
        return CreatedBlob(
            id = json.requiredString("id"),
            deleteToken = json.requiredString("delete_token"),
            expiresAt = json.requiredLong("expires_at").toULong(),
            maxPartBytes = json.requiredLong("max_part_bytes").also { require(it in 1..MAX_PART_BYTES.toLong()) }.toInt(),
        )
    }

    fun uploadPart(blobId: String, index: Int, deleteToken: String, bytes: ByteArray) {
        require(index >= 0 && bytes.isNotEmpty() && bytes.size <= MAX_PART_BYTES)
        requestWithRetry("PUT", blobPath(blobId, "parts/$index"), mapOf("Authorization" to bearer(deleteToken)), bytes)
    }

    fun completeUpload(blobId: String, deleteToken: String) {
        request("POST", blobPath(blobId, "complete"), mapOf("Authorization" to bearer(deleteToken)))
    }

    fun getBlob(blobId: String): BlobMetadata {
        val json = jsonRequest("GET", blobPath(blobId))
        return BlobMetadata(
            size = json.requiredLong("size").toULong(),
            parts = json.requiredLong("parts").also { require(it in 1..Int.MAX_VALUE.toLong()) }.toInt(),
            burn = json.requiredBoolean("burn"),
            expiresAt = json.requiredLong("expires_at").toULong(),
            state = json.requiredString("state"),
        )
    }

    fun claimBlob(blobId: String): BlobClaim {
        val json = jsonRequest("POST", blobPath(blobId, "claim"))
        val token = json.optString("claim").takeIf { it.isNotEmpty() && it != "null" }
        val expires = if (json.isNull("claim_expires_at")) null else json.requiredLong("claim_expires_at").toULong()
        return BlobClaim(token, expires)
    }

    /** Downloads one bounded ciphertext part; decrypt and persist it before requesting the next. */
    fun downloadPart(blobId: String, index: Int, claimToken: String? = null): ByteArray {
        require(index >= 0)
        val headers = claimToken?.let { mapOf("X-Claim" to it) }.orEmpty()
        return requestWithRetry("GET", blobPath(blobId, "parts/$index"), headers, responseLimit = MAX_PART_BYTES)
    }

    /**
     * Finishing is safe to retry. If a burn was deleted by an earlier attempt whose response
     * was lost, the same claim gets a not-found response; after an ambiguous network/5xx result
     * that confirms the earlier finish completed.
     */
    fun finishDownload(blobId: String, claimToken: String?, burn: Boolean) {
        val headers = claimToken?.let { mapOf("X-Claim" to it) }.orEmpty()
        var previousAttemptMayHaveCommitted = false
        var lastError: IOException? = null
        repeat(MAX_IDEMPOTENT_ATTEMPTS) { attempt ->
            try {
                request("POST", blobPath(blobId, "finish"), headers)
                return
            } catch (error: RelayHttpException) {
                if (burn && previousAttemptMayHaveCommitted && (error.status == 404 || error.status == 410)) return
                if (error.status < 500 && error.status != 429) throw error
                lastError = error
                if (error.status >= 500) previousAttemptMayHaveCommitted = true
            } catch (error: IOException) {
                lastError = error
                previousAttemptMayHaveCommitted = true
            }
            if (attempt + 1 < MAX_IDEMPOTENT_ATTEMPTS) waitBeforeRetry(attempt)
        }
        throw lastError ?: IOException("Relay finish request failed")
    }

    fun deleteBlob(blobId: String, deleteToken: String) {
        request("DELETE", blobPath(blobId), mapOf("Authorization" to bearer(deleteToken)))
    }

    internal fun endpoint(path: String): String {
        require(path.startsWith('/') && !path.startsWith("//"))
        return origin + path
    }

    private fun blobPath(blobId: String, suffix: String? = null): String {
        require(blobId.matches(BLOB_ID_PATTERN))
        return "/v1/blobs/$blobId" + (suffix?.let { "/$it" } ?: "")
    }

    private fun bearer(token: String): String {
        require(token.isNotBlank() && '\r' !in token && '\n' !in token)
        return "Bearer $token"
    }

    private fun jsonRequest(
        method: String,
        path: String,
        headers: Map<String, String> = emptyMap(),
        body: ByteArray? = null,
    ): JSONObject {
        val bytes = request(method, path, headers, body, responseLimit = MAX_JSON_BYTES)
        return try {
            JSONObject(bytes.toString(Charsets.UTF_8))
        } catch (error: Exception) {
            throw RelayProtocolException("Relay returned an invalid JSON response", error)
        }
    }

    private fun requestWithRetry(
        method: String,
        path: String,
        headers: Map<String, String> = emptyMap(),
        body: ByteArray? = null,
        responseLimit: Int = MAX_JSON_BYTES,
    ): ByteArray {
        var lastError: IOException? = null
        repeat(MAX_IDEMPOTENT_ATTEMPTS) { attempt ->
            try {
                return request(method, path, headers, body, responseLimit)
            } catch (error: RelayProtocolException) {
                throw error
            } catch (error: RelayHttpException) {
                if (error.status < 500 && error.status != 429) throw error
                lastError = error
                if (attempt + 1 < MAX_IDEMPOTENT_ATTEMPTS) {
                    waitBeforeRetry(attempt)
                }
            } catch (error: IOException) {
                lastError = error
                if (attempt + 1 < MAX_IDEMPOTENT_ATTEMPTS) {
                    waitBeforeRetry(attempt)
                }
            }
        }
        throw lastError ?: IOException("Relay request failed")
    }

    private fun waitBeforeRetry(attempt: Int) {
        try {
            Thread.sleep(RETRY_DELAY_MS shl attempt)
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            throw IOException("Relay request interrupted", interrupted)
        }
    }

    private fun request(
        method: String,
        path: String,
        headers: Map<String, String> = emptyMap(),
        body: ByteArray? = null,
        responseLimit: Int = MAX_JSON_BYTES,
    ): ByteArray {
        val connection = connectionFactory(URL(endpoint(path)))
        try {
            connection.requestMethod = method
            connection.connectTimeout = CONNECT_TIMEOUT_MS
            connection.readTimeout = READ_TIMEOUT_MS
            connection.useCaches = false
            connection.instanceFollowRedirects = false
            connection.setRequestProperty("Accept", "application/json")
            connection.setRequestProperty("Cache-Control", "no-store")
            headers.forEach(connection::setRequestProperty)
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty(
                    "Content-Type",
                    if (method == "PUT") "application/octet-stream" else "application/json; charset=utf-8",
                )
                connection.setFixedLengthStreamingMode(body.size)
                connection.outputStream.use { it.write(body) }
            }
            val status = connection.responseCode
            if (status !in 200..299) throw RelayHttpException(status)
            return connection.inputStream.use { input ->
                val output = ByteArrayOutputStream(minOf(responseLimit, connection.contentLength.takeIf { it > 0 } ?: 8192))
                val buffer = ByteArray(16 * 1024)
                var total = 0
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    total += count
                    if (total > responseLimit) throw RelayProtocolException("Relay response exceeds its size limit")
                    output.write(buffer, 0, count)
                }
                output.toByteArray()
            }
        } finally {
            connection.disconnect()
        }
    }

    companion object {
        const val MAX_PART_BYTES = 1_048_593
        private const val MAX_JSON_BYTES = 64 * 1024
        private const val MAX_TTL_SECONDS = 43_200L
        private const val CONNECT_TIMEOUT_MS = 15_000
        private const val READ_TIMEOUT_MS = 60_000
        private const val MAX_IDEMPOTENT_ATTEMPTS = 5
        private const val RETRY_DELAY_MS = 250L
        private val BLOB_ID_PATTERN = Regex("[A-Za-z0-9_-]{22}")

        fun fromRelayHost(host: String): RelayClient {
            require(host.isNotBlank() && '/' !in host && '@' !in host && '#' !in host && '?' !in host)
            return RelayClient("https://$host")
        }

        private fun validateOrigin(value: String): String {
            val uri = runCatching { URI(value) }.getOrElse { throw IllegalArgumentException("Invalid relay URL", it) }
            require(uri.scheme.equals("https", ignoreCase = true)) { "Relay connections must use HTTPS" }
            require(uri.host != null && uri.rawUserInfo == null && uri.rawQuery == null && uri.rawFragment == null)
            require(uri.rawPath.isNullOrEmpty() || uri.rawPath == "/") { "Relay URL must be an origin without a path" }
            return "https://${uri.rawAuthority}"
        }
    }
}

class RelayHttpException(val status: Int) : IOException("Relay request failed with HTTP $status")

class RelayProtocolException(message: String, cause: Throwable? = null) : IOException(message, cause)

private fun JSONObject.requiredString(name: String): String =
    optString(name).takeIf { it.isNotEmpty() && it != "null" }
        ?: throw RelayProtocolException("Relay response is missing $name")

private fun JSONObject.requiredLong(name: String): Long {
    val value = opt(name)
    val result = when (value) {
        is Number -> value.toLong()
        is String -> value.toLongOrNull()
        null -> null
        else -> null
    }
    return result?.takeIf { it >= 0 } ?: throw RelayProtocolException("Relay response has invalid $name")
}

private fun JSONObject.requiredBoolean(name: String): Boolean =
    opt(name) as? Boolean ?: throw RelayProtocolException("Relay response has invalid $name")
