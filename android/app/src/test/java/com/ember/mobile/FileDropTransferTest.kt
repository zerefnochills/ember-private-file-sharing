package com.ember.mobile

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream

class FileDropTransferTest {
    @get:Rule
    val temp = TemporaryFolder()

    @Test
    fun smallJpegMetadataIsStrippedAndReportedBeforeSend() {
        val jpeg = ByteArrayOutputStream().apply {
            write(byteArrayOf(0xff.toByte(), 0xd8.toByte(), 0xff.toByte(), 0xe1.toByte()))
            val exif = "Exif\u0000\u0000secret".toByteArray()
            write(byteArrayOf(0, (exif.size + 2).toByte()))
            write(exif)
            write(byteArrayOf(0xff.toByte(), 0xda.toByte(), 0, 8, 1, 1, 0, 0, 1, 2, 3, 0xff.toByte(), 0xd9.toByte()))
        }.toByteArray()

        val prepared = ByteArrayInputStream(jpeg).use { input ->
            FileDropTransfer.prepareForSend(
                input, "photo.jpg", "image/jpeg", jpeg.size.toLong(), temp.root,
            )
        }
        prepared.use { file ->
            assertEquals("stripped", file.summary.status)
            assertTrue(file.summary.fieldsRemoved.any { it.contains("EXIF metadata") })
            assertTrue(file.summary.userMessage.contains("Removed"))
            assertTrue(file.file.length() < jpeg.size)
            assertTrue(!file.file.readBytes().toString(Charsets.ISO_8859_1).contains("secret"))
        }
        assertTrue(temp.root.listFiles().orEmpty().isEmpty())
    }

    @Test
    fun unsupportedAndLargeFilesHaveVisiblePassThroughWarnings() {
        val opaque = "not a supported format".toByteArray()
        val unsupported = ByteArrayInputStream(opaque).use { input ->
            FileDropTransfer.prepareForSend(input, "data.bin", "application/octet-stream", opaque.size.toLong(), temp.root)
        }
        unsupported.use { file ->
            assertEquals("unsupported", file.summary.status)
            assertTrue(file.summary.requiresExplicitPassThrough)
            assertEquals(opaque.toList(), file.file.readBytes().toList())
        }

        val large = ByteArray(8 * 1024 * 1024 + 1) { 7 }
        val skipped = ByteArrayInputStream(large).use { input ->
            FileDropTransfer.prepareForSend(input, "large.bin", "application/octet-stream", large.size.toLong(), temp.root)
        }
        skipped.use { file ->
            assertEquals("skipped", file.summary.status)
            assertTrue(file.summary.requiresExplicitPassThrough)
            assertEquals(large.size.toLong(), file.file.length())
            assertTrue(file.summary.reason.contains("8 MiB"))
        }
        assertTrue(temp.root.listFiles().orEmpty().isEmpty())
        large.fill(0)
    }

    @Test
    fun streamsLargeFileThroughRelayAndOnlyReturnsAfterFinalVerification() {
        val server = FakeRelayServer(burn = true)
        val host = "relay.ember.example"
        val bytes = ByteArray(1_048_576 + 91) { (it % 239).toByte() }
        val stages = mutableListOf<String>()

        val link = ByteArrayInputStream(bytes).use { input ->
            FileDropTransfer.send(
                relayHost = host,
                input = input,
                name = "private-data.bin",
                mime = "application/octet-stream",
                size = bytes.size.toLong(),
                passphrase = null,
                burn = true,
                ttlSeconds = 3_600,
                relay = server.client(host),
                onProgress = stages::add,
            )
        }
        assertTrue(server.expectedParts >= 3)
        assertTrue(server.parts.values.all { it.isNotEmpty() && it.size <= RelayClient.MAX_PART_BYTES })
        assertTrue(stages.any { it.startsWith("Encrypting and uploading") })

        val prepared = FileDropTransfer.prepareReceive(link, null) { server.client(it) }
        val verified = prepared.use { receive ->
            FileDropTransfer.receive(receive, temp.root)
        }
        verified.use { result ->
            assertEquals("private-data.bin", result.metadata.name)
            assertEquals(bytes.size.toULong(), result.metadata.size)
            assertEquals(true, result.metadata.burn)
            assertArrayEquals(bytes, result.file.readBytes())
        }
        assertEquals(1, server.claimCalls)
        assertEquals(1, server.finishCalls)
        assertTrue(temp.root.listFiles().orEmpty().isEmpty())
    }

    @Test
    fun tamperedFileIsNeverFinishedAndPrivateTempOutputIsDeleted() {
        val server = FakeRelayServer(burn = false)
        val host = "relay.ember.example"
        val bytes = ByteArray(1_100_000) { (it % 199).toByte() }
        val link = ByteArrayInputStream(bytes).use { input ->
            FileDropTransfer.send(
                host,
                input,
                "tamper.bin",
                "application/octet-stream",
                bytes.size.toLong(),
                null,
                false,
                relay = server.client(host),
            )
        }
        server.tamperPartIndex = server.expectedParts - 1
        val prepared = FileDropTransfer.prepareReceive(link, null) { server.client(it) }

        try {
            prepared.use { FileDropTransfer.receive(it, temp.root) }
            throw AssertionError("tampered ciphertext must fail authentication")
        } catch (_: Exception) {
            // Authentication and cleanup are the expected result.
        }
        assertEquals(0, server.finishCalls)
        assertTrue(temp.root.listFiles().orEmpty().isEmpty())
    }
}
