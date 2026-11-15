package com.ember.mobile

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream

class FileDropTransferTest {
    @get:Rule
    val temp = TemporaryFolder()

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
