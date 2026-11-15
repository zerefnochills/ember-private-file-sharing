package com.ember.mobile

import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.inspectLink
import com.ember.mobile.core.solvePow
import java.io.ByteArrayOutputStream

data class PreparedTextReceive(
    val crypto: EmberCrypto,
    val relay: RelayClient,
    val blobId: String,
    val metadata: BlobMetadata,
) : AutoCloseable {
    override fun close() = crypto.close()
}

/** M10 bounded text flow. File payloads wait for the core streaming API. */
object TextDropTransfer {
    const val MAX_TEXT_BYTES = 1_048_576
    private const val PART0_SIZE = 4_137
    private const val CHUNK_CIPHERTEXT_SIZE = RelayClient.MAX_PART_BYTES
    private const val MAX_TEXT_CIPHERTEXT_SIZE = PART0_SIZE + CHUNK_CIPHERTEXT_SIZE
    private const val MAX_TTL_SECONDS = 43_200L

    fun send(
        relayHost: String,
        text: String,
        passphrase: String?,
        burn: Boolean,
        ttlSeconds: Long = 3_600,
        onProgress: (String) -> Unit = {},
    ): String = sendWithRelay(
        relayHost,
        text,
        passphrase,
        burn,
        ttlSeconds,
        RelayClient.fromRelayHost(relayHost.trim()),
        onProgress,
    )

    internal fun sendWithRelay(
        relayHost: String,
        text: String,
        passphrase: String?,
        burn: Boolean,
        ttlSeconds: Long,
        relay: RelayClient,
        onProgress: (String) -> Unit = {},
    ): String {
        require(ttlSeconds in 1..MAX_TTL_SECONDS)
        val plaintextBytes = text.toByteArray(Charsets.UTF_8)
        try {
            require(plaintextBytes.isNotEmpty()) { "Enter some text to send" }
            require(plaintextBytes.size <= MAX_TEXT_BYTES) { "Text is limited to 1 MiB until file streaming is available" }
        } finally {
            plaintextBytes.fill(0)
        }

        val now = System.currentTimeMillis() / 1_000
        val crypto = EmberCrypto.generate()
        try {
            val ciphertext = crypto.encryptTextWithOptions(
                text,
                burn,
                now.toULong(),
                (now + ttlSeconds).toULong(),
            )
            val parts = try {
                splitCiphertext(ciphertext)
            } finally {
                ciphertext.fill(0)
            }
            try {
                onProgress("Requesting proof-of-work challenge…")
                val challenge = relay.getPowChallenge()
                onProgress("Solving proof of work…")
                val nonce = solvePow(challenge.challenge, challenge.difficulty)
                val totalCiphertextBytes = parts.sumOf { it.size.toLong() }.toULong()
                onProgress("Creating encrypted drop…")
                val created = relay.createBlob(
                    size = totalCiphertextBytes,
                    parts = parts.size,
                    burn = burn,
                    ttlSeconds = ttlSeconds,
                    challenge = challenge.challenge,
                    nonce = nonce,
                )
                try {
                    parts.forEachIndexed { index, part ->
                        onProgress("Uploading part ${index + 1} of ${parts.size}…")
                        relay.uploadPart(created.id, index, created.deleteToken, part)
                    }
                    onProgress("Finalizing upload…")
                    relay.completeUpload(created.id, created.deleteToken)
                    onProgress("Building secure share link…")
                    return if (passphrase.isNullOrEmpty()) {
                        crypto.buildLink(relayHost.trim(), created.id)
                    } else {
                        crypto.buildProtectedLink(relayHost.trim(), created.id, passphrase)
                    }
                } catch (error: Exception) {
                    runCatching { relay.deleteBlob(created.id, created.deleteToken) }
                    throw error
                }
            } finally {
                parts.forEach { it.fill(0) }
            }
        } finally {
            crypto.close()
        }
    }

    fun prepareReceive(link: String, passphrase: String?): PreparedTextReceive =
        prepareReceive(link, passphrase) { RelayClient.fromRelayHost(it) }

    internal fun prepareReceive(
        link: String,
        passphrase: String?,
        relayFactory: (String) -> RelayClient,
    ): PreparedTextReceive {
        val info = inspectLink(link.trim())
        val crypto = EmberCrypto.fromLink(link.trim(), passphrase?.takeIf(String::isNotEmpty))
        try {
            val relay = relayFactory(info.host)
            val metadata = relay.getBlob(info.blobId)
            require(metadata.state == "available") { "This drop is not ready to receive" }
            require(metadata.parts == 2 && metadata.size <= MAX_TEXT_CIPHERTEXT_SIZE.toULong()) {
                "This drop is too large for the text receiver; file streaming is not available yet"
            }
            return PreparedTextReceive(crypto, relay, info.blobId, metadata)
        } catch (error: Exception) {
            crypto.close()
            throw error
        }
    }

    /** Call only after the user has confirmed the burn-after-view warning. */
    fun receive(prepared: PreparedTextReceive, onProgress: (String) -> Unit = {}): String {
        onProgress(if (prepared.metadata.burn) "Claiming one-view drop…" else "Starting download…")
        val claim = prepared.relay.claimBlob(prepared.blobId)
        val ciphertext = ZeroizingByteArrayOutputStream(prepared.metadata.size.toInt())
        try {
            repeat(prepared.metadata.parts) { index ->
                onProgress("Downloading part ${index + 1} of ${prepared.metadata.parts}…")
                val part = prepared.relay.downloadPart(prepared.blobId, index, claim.token)
                try {
                    if (part.isEmpty() || part.size > RelayClient.MAX_PART_BYTES ||
                        ciphertext.size().toLong() + part.size > prepared.metadata.size.toLong()
                    ) {
                        throw RelayProtocolException("Relay returned invalid ciphertext parts")
                    }
                    ciphertext.write(part)
                } finally {
                    part.fill(0)
                }
            }
            if (ciphertext.size().toULong() != prepared.metadata.size) {
                throw RelayProtocolException("Relay ciphertext size does not match metadata")
            }

            val encrypted = ciphertext.toByteArray()
            try {
                onProgress("Authenticating encrypted text…")
                val drop = prepared.crypto.decryptDropPayload(encrypted)
                val result: String
                try {
                    if (drop.kind != "text") throw UnsupportedDropException()
                    if (drop.burn != prepared.metadata.burn || drop.content.size.toULong() != drop.size) {
                        throw RelayProtocolException("Encrypted drop metadata does not match the relay")
                    }
                    result = drop.content.decodeToString(throwOnInvalidSequence = true)
                } finally {
                    drop.content.fill(0)
                }
                onProgress("Finishing verified download…")
                prepared.relay.finishDownload(prepared.blobId, claim.token)
                return result
            } finally {
                encrypted.fill(0)
            }
        } finally {
            ciphertext.clear()
        }
    }

    internal fun splitCiphertext(ciphertext: ByteArray): List<ByteArray> {
        require(ciphertext.size > PART0_SIZE)
        val parts = ArrayList<ByteArray>(2)
        parts += ciphertext.copyOfRange(0, PART0_SIZE)
        var offset = PART0_SIZE
        while (offset < ciphertext.size) {
            val end = minOf(offset + CHUNK_CIPHERTEXT_SIZE, ciphertext.size)
            parts += ciphertext.copyOfRange(offset, end)
            offset = end
        }
        require(parts.size == 2 && parts[1].size <= RelayClient.MAX_PART_BYTES)
        return parts
    }
}

class UnsupportedDropException : IllegalStateException("This Android build can receive text only")

private class ZeroizingByteArrayOutputStream(size: Int) : ByteArrayOutputStream(size) {
    fun clear() {
        buf.fill(0)
        reset()
    }
}
