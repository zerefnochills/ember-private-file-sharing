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
    ): String {
        require(ttlSeconds in 1..MAX_TTL_SECONDS)
        val plaintextBytes = text.toByteArray(Charsets.UTF_8)
        try {
            require(plaintextBytes.isNotEmpty()) { "Enter some text to send" }
            require(plaintextBytes.size <= MAX_TEXT_BYTES) { "Text is limited to 1 MiB until file streaming is available" }
        } finally {
            plaintextBytes.fill(0)
        }

        val relay = RelayClient.fromRelayHost(relayHost.trim())
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
                val challenge = relay.getPowChallenge()
                val nonce = solvePow(challenge.challenge, challenge.difficulty)
                val totalCiphertextBytes = parts.sumOf { it.size.toLong() }.toULong()
                val created = relay.createBlob(
                    size = totalCiphertextBytes,
                    parts = parts.size,
                    burn = burn,
                    ttlSeconds = ttlSeconds,
                    challenge = challenge.challenge,
                    nonce = nonce,
                )
                try {
                    parts.forEachIndexed { index, part -> relay.uploadPart(created.id, index, created.deleteToken, part) }
                    relay.completeUpload(created.id, created.deleteToken)
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
            plaintextBytes.fill(0)
            crypto.close()
        }
    }

    fun prepareReceive(link: String, passphrase: String?): PreparedTextReceive {
        val info = inspectLink(link.trim())
        val crypto = EmberCrypto.fromLink(link.trim(), passphrase?.takeIf(String::isNotEmpty))
        try {
            val relay = RelayClient.fromRelayHost(info.host)
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
    fun receive(prepared: PreparedTextReceive): String {
        val claim = prepared.relay.claimBlob(prepared.blobId)
        val ciphertext = ZeroizingByteArrayOutputStream(prepared.metadata.size.toInt())
        try {
            repeat(prepared.metadata.parts) { index ->
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
                val drop = prepared.crypto.decryptDropPayload(encrypted)
                val result: String
                try {
                    if (drop.kind != "text" || drop.burn != prepared.metadata.burn ||
                        drop.content.size.toULong() != drop.size
                    ) {
                        throw RelayProtocolException("Encrypted drop metadata does not match the relay")
                    }
                    result = drop.content.decodeToString(throwOnInvalidSequence = true)
                } finally {
                    drop.content.fill(0)
                }
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

private class ZeroizingByteArrayOutputStream(size: Int) : ByteArrayOutputStream(size) {
    fun clear() {
        buf.fill(0)
        reset()
    }
}
