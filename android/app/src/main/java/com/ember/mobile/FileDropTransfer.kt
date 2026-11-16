package com.ember.mobile

import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.EmberDecryptStream
import com.ember.mobile.core.EmberEncryptStream
import com.ember.mobile.core.inspectLink
import com.ember.mobile.core.solvePow
import com.ember.mobile.core.stripFileMetadata
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.io.RandomAccessFile

data class PreparedFileReceive(
    val crypto: EmberCrypto,
    val relay: RelayClient,
    val blobId: String,
    val metadata: BlobMetadata,
    var activeClaim: BlobClaim? = null,
) : AutoCloseable {
    override fun close() {
        activeClaim = null
        crypto.close()
    }
}

data class VerifiedFileReceive(val file: File, val metadata: FileMetadata) : AutoCloseable {
    override fun close() = FileDropTransfer.wipeAndDelete(file)
}

data class FileMetadata(
    val name: String,
    val mime: String,
    val size: ULong,
    val burn: Boolean,
)

data class PreparedFileSend(
    val file: File,
    val name: String,
    val mime: String,
    val size: Long,
    val summary: MetadataStripSummary,
) : AutoCloseable {
    override fun close() = FileDropTransfer.wipeAndDelete(file)
}

data class MetadataStripSummary(
    val status: String,
    val format: String,
    val fieldsRemoved: List<String>,
    val reason: String,
) {
    val requiresExplicitPassThrough: Boolean get() = status == "unsupported" || status == "skipped"
}

/** Streams file bytes through the Rust core; Android only handles bounded relay parts and I/O. */
object FileDropTransfer {
    private const val CHUNK_BYTES = 1_048_576
    private const val PART0_BYTES = 4_137
    private const val PART_OVERHEAD_BYTES = RelayClient.MAX_PART_BYTES - CHUNK_BYTES
    private const val PADDING_BAND_BYTES = 65_536L
    private const val MIN_PADDED_BYTES = 4_096L
    private const val MAX_CIPHERTEXT_BYTES = 52_433_787L
    private const val MAX_TTL_SECONDS = 43_200L
    private const val MAX_STRIP_INPUT_BYTES = 8 * 1024 * 1024

    /** Strips supported small files with core; larger files are staged by bounded I/O and flagged. */
    fun prepareForSend(
        input: InputStream,
        name: String,
        mime: String,
        size: Long,
        cacheDir: File,
    ): PreparedFileSend {
        require(size > 0) { "Choose a non-empty file" }
        require(name.isNotBlank() && name.toByteArray(Charsets.UTF_8).size <= 255) { "File name is invalid" }
        require(mime.toByteArray(Charsets.UTF_8).size <= 127) { "File type is invalid" }
        val staged = File.createTempFile("ember-send-", ".tmp", cacheDir).apply {
            setReadable(false, false)
            setWritable(false, false)
            setReadable(true, true)
            setWritable(true, true)
        }
        var succeeded = false
        try {
            val summary = if (size <= MAX_STRIP_INPUT_BYTES) {
                require(size <= Int.MAX_VALUE)
                val original = ByteArray(size.toInt())
                try {
                    readExactInput(input, original)
                    val report = stripFileMetadata(original, name)
                    try {
                        staged.outputStream().use { it.write(report.content) }
                        MetadataStripSummary(
                            status = report.status,
                            format = report.format,
                            fieldsRemoved = report.fieldsRemoved,
                            reason = report.reason,
                        )
                    } finally {
                        report.content.fill(0)
                    }
                } finally {
                    original.fill(0)
                }
            } else {
                staged.outputStream().use { output -> copyExactInput(input, output, size) }
                MetadataStripSummary(
                    status = "skipped",
                    format = "over-budget file",
                    fieldsRemoved = emptyList(),
                    reason = "File exceeds the 8 MiB metadata analysis cap; bytes were staged without loading the whole file.",
                )
            }
            staged.setLastModified(0L)
            succeeded = true
            return PreparedFileSend(staged, name, mime, staged.length(), summary)
        } finally {
            if (!succeeded) wipeAndDelete(staged)
        }
    }

    fun send(
        relayHost: String,
        input: InputStream,
        name: String,
        mime: String,
        size: Long,
        passphrase: String?,
        burn: Boolean,
        ttlSeconds: Long = 3_600,
        relay: RelayClient = RelayClient.fromRelayHost(relayHost.trim()),
        onProgress: (String) -> Unit = {},
    ): String {
        require(size > 0) { "Choose a non-empty file" }
        require(ttlSeconds in 1..MAX_TTL_SECONDS)
        require(name.isNotBlank() && name.toByteArray(Charsets.UTF_8).size <= 255) { "File name is invalid" }
        require(mime.toByteArray(Charsets.UTF_8).size <= 127) { "File type is invalid" }
        val layout = encryptedLayout(size.toULong())
        require(layout.first <= MAX_CIPHERTEXT_BYTES.toULong()) { "This file exceeds the relay's size limit" }

        val now = System.currentTimeMillis() / 1_000
        val crypto = EmberCrypto.generate()
        val stream = EmberEncryptStream(
            crypto, name, mime, size.toULong(), burn, now.toULong(), (now + ttlSeconds).toULong(),
        )
        var created: CreatedBlob? = null
        try {
            onProgress("Requesting proof-of-work challenge…")
            val challenge = relay.getPowChallenge()
            onProgress("Solving proof of work…")
            val nonce = solvePow(challenge.challenge, challenge.difficulty)
            onProgress("Creating encrypted file drop…")
            val newBlob = relay.createBlob(
                layout.first, layout.second, burn, ttlSeconds, challenge.challenge, nonce,
            )
            created = newBlob

            val part0 = stream.takePart0()
            try {
                onProgress("Uploading part 1 of ${layout.second}…")
                relay.uploadPart(newBlob.id, 0, newBlob.deleteToken, part0)
            } finally {
                part0.fill(0)
            }

            val buffer = ByteArray(64 * 1024)
            var totalRead = 0L
            var partIndex = 1
            try {
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    if (count == 0) continue
                    totalRead = Math.addExact(totalRead, count.toLong())
                    require(totalRead <= size) { "File changed while it was being read" }
                    val encryptedParts = stream.update(buffer.copyOf(count))
                    buffer.fill(0, 0, count)
                    for (part in encryptedParts) {
                        try {
                            onProgress("Encrypting and uploading part ${partIndex + 1} of ${layout.second}…")
                            relay.uploadPart(newBlob.id, partIndex++, newBlob.deleteToken, part)
                        } finally {
                            part.fill(0)
                        }
                    }
                }
                require(totalRead == size) { "File changed while it was being read" }
                val finalParts = stream.finish()
                for (part in finalParts) {
                    try {
                        onProgress("Uploading final part ${partIndex + 1} of ${layout.second}…")
                        relay.uploadPart(newBlob.id, partIndex++, newBlob.deleteToken, part)
                    } finally {
                        part.fill(0)
                    }
                }
            } finally {
                buffer.fill(0)
            }
            check(partIndex == layout.second) { "Encrypted file part count did not match the relay layout" }
            onProgress("Finalizing encrypted upload…")
            relay.completeUpload(newBlob.id, newBlob.deleteToken)
            return if (passphrase.isNullOrEmpty()) {
                crypto.buildLink(relayHost.trim(), newBlob.id)
            } else {
                crypto.buildProtectedLink(relayHost.trim(), newBlob.id, passphrase)
            }
        } catch (error: Exception) {
            created?.let { runCatching { relay.deleteBlob(it.id, it.deleteToken) } }
            throw error
        } finally {
            stream.close()
            crypto.close()
        }
    }

    fun prepareReceive(link: String, passphrase: String?): PreparedFileReceive =
        prepareReceive(link, passphrase) { RelayClient.fromRelayHost(it) }

    internal fun prepareReceive(
        link: String,
        passphrase: String?,
        relayFactory: (String) -> RelayClient,
    ): PreparedFileReceive {
        val info = inspectLink(link.trim())
        val crypto = EmberCrypto.fromLink(link.trim(), passphrase?.takeIf(String::isNotEmpty))
        try {
            val relay = relayFactory(info.host)
            val metadata = relay.getBlob(info.blobId)
            require(metadata.state == "available") { "This drop is not ready to receive" }
            require(metadata.parts >= 2 && metadata.size in (PART0_BYTES + 1).toULong()..MAX_CIPHERTEXT_BYTES.toULong()) {
                "This drop does not have a valid file size"
            }
            return PreparedFileReceive(crypto, relay, info.blobId, metadata)
        } catch (error: Exception) {
            crypto.close()
            throw error
        }
    }

    /** Decrypted bytes stay in app-private cache until core.finish() authenticates the final tag. */
    fun receive(prepared: PreparedFileReceive, cacheDir: File, onProgress: (String) -> Unit = {}): VerifiedFileReceive {
        onProgress(
            when {
                prepared.activeClaim?.token != null -> "Resuming claimed file download…"
                prepared.metadata.burn -> "Claiming one-view file…"
                else -> "Starting file download…"
            },
        )
        val claim = prepared.activeClaim?.takeIf { it.token != null }
            ?: prepared.relay.claimBlob(prepared.blobId).also { prepared.activeClaim = it }
        if (prepared.metadata.burn && claim.token == null) {
            throw RelayProtocolException("Relay did not issue a one-view claim token")
        }
        val temp = File.createTempFile("ember-verified-", ".tmp", cacheDir).apply {
            setReadable(false, false)
            setWritable(false, false)
            setReadable(true, true)
            setWritable(true, true)
        }
        var decryptor: EmberDecryptStream? = null
        var succeeded = false
        try {
            val part0 = prepared.relay.downloadPart(prepared.blobId, 0, claim.token)
            try {
                decryptor = EmberDecryptStream(prepared.crypto, part0)
            } finally {
                part0.fill(0)
            }
            val fileMeta = decryptor.metadata()
            require(fileMeta.kind == "file") { "This drop contains text, not a file" }
            require(fileMeta.size > 0uL && fileMeta.burn == prepared.metadata.burn) {
                "Encrypted file metadata does not match the relay"
            }
            val layout = encryptedLayout(fileMeta.size)
            require(layout.first == prepared.metadata.size && layout.second == prepared.metadata.parts) {
                "Encrypted file size or part count does not match the relay"
            }

            var written = 0uL
            var sawFinal = false
            FileOutputStream(temp).use { output ->
                for (index in 1 until prepared.metadata.parts) {
                    onProgress("Downloading and verifying part ${index + 1} of ${prepared.metadata.parts}…")
                    val ciphertext = prepared.relay.downloadPart(prepared.blobId, index, claim.token)
                    try {
                        val chunk = decryptor.pushPart(ciphertext)
                        try {
                            require(!sawFinal) { "Relay returned a part after the final chunk" }
                            written = Math.addExact(written.toLong(), chunk.plaintext.size.toLong()).toULong()
                            require(written <= fileMeta.size) { "Decrypted file exceeds its declared size" }
                            output.write(chunk.plaintext)
                            sawFinal = chunk.isFinal
                            require(!sawFinal || index == prepared.metadata.parts - 1) {
                                "Relay returned trailing parts after the final chunk"
                            }
                        } finally {
                            chunk.plaintext.fill(0)
                        }
                    } finally {
                        ciphertext.fill(0)
                    }
                }
                output.fd.sync()
            }
            require(sawFinal && written == fileMeta.size) { "Encrypted file ended before its declared size" }
            onProgress("Authenticating final file chunk…")
            decryptor.finish()
            onProgress("Finishing verified download…")
            prepared.relay.finishDownload(prepared.blobId, claim.token, prepared.metadata.burn)
            prepared.activeClaim = null
            succeeded = true
            return VerifiedFileReceive(
                temp,
                FileMetadata(fileMeta.safeName, fileMeta.mime, fileMeta.size, fileMeta.burn),
            )
        } finally {
            decryptor?.close()
            if (!succeeded) wipeAndDelete(temp)
        }
    }

    private fun encryptedLayout(size: ULong): Pair<ULong, Int> {
        require(size > 0uL && size <= Long.MAX_VALUE.toULong())
        val bytes = size.toLong()
        val padded = if (bytes <= MIN_PADDED_BYTES) MIN_PADDED_BYTES else {
            Math.multiplyExact(Math.floorDiv(Math.addExact(bytes, PADDING_BAND_BYTES - 1), PADDING_BAND_BYTES), PADDING_BAND_BYTES)
        }
        val chunks = Math.toIntExact((padded + CHUNK_BYTES - 1) / CHUNK_BYTES)
        val total = Math.addExact(PART0_BYTES.toLong(), Math.addExact(padded, Math.multiplyExact(chunks.toLong(), PART_OVERHEAD_BYTES.toLong())))
        return total.toULong() to Math.addExact(chunks, 1)
    }

    private fun readExactInput(input: InputStream, target: ByteArray) {
        var offset = 0
        while (offset < target.size) {
            val count = input.read(target, offset, target.size - offset)
            if (count < 0) throw IllegalArgumentException("File changed while it was being read")
            if (count > 0) offset += count
        }
        require(input.read() < 0) { "File changed while it was being read" }
    }

    private fun copyExactInput(input: InputStream, output: java.io.OutputStream, size: Long) {
        val buffer = ByteArray(64 * 1024)
        var remaining = size
        try {
            while (remaining > 0) {
                val count = input.read(buffer, 0, minOf(buffer.size.toLong(), remaining).toInt())
                if (count < 0) throw IllegalArgumentException("File changed while it was being read")
                if (count == 0) continue
                output.write(buffer, 0, count)
                remaining -= count
                buffer.fill(0, 0, count)
            }
            require(input.read() < 0) { "File changed while it was being read" }
        } finally {
            buffer.fill(0)
        }
    }

    internal fun wipeAndDelete(file: File) {
        if (!file.exists()) return
        runCatching {
            RandomAccessFile(file, "rw").use { output ->
                val zeros = ByteArray(64 * 1024)
                var remaining = output.length()
                output.seek(0)
                while (remaining > 0) {
                    val count = minOf(zeros.size.toLong(), remaining).toInt()
                    output.write(zeros, 0, count)
                    remaining -= count
                }
                zeros.fill(0)
                output.fd.sync()
            }
        }
        file.delete()
    }
}
