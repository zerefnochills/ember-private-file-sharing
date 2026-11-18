package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

@RunWith(AndroidJUnit4::class)
class M10TransferStressInstrumentedTest {
    @Test
    fun twentyMiBFileSendAndReceiveRetriesOnTransientRelayErrors() = transferRoundTrip(20L * MIB, injectRetry = true)

    @Test
    fun fiftyMiBFileSendAndReceiveStaysStreaming() = transferRoundTrip(50L * MIB, injectRetry = false)

    private fun transferRoundTrip(size: Long, injectRetry: Boolean) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File.createTempFile("m10-input-", ".bin", context.cacheDir)
        val bytes = ByteArray(64 * 1024) { (it % 239).toByte() }
        val expectedDigest = MessageDigest.getInstance("SHA-256")
        try {
            FileOutputStream(source).use { output ->
                var remaining = size
                while (remaining > 0) {
                    val count = minOf(bytes.size.toLong(), remaining).toInt()
                    output.write(bytes, 0, count)
                    expectedDigest.update(bytes, 0, count)
                    remaining -= count
                }
            }

            val host = "10.0.2.2:8080"
            val relay = relay(host)
            if (injectRetry) armTransientFailure(host)
            val link = FileInputStream(source).use { input ->
                FileDropTransfer.send(
                    relayHost = host,
                    input = input,
                    name = "m10-stress.bin",
                    mime = "application/octet-stream",
                    size = size,
                    passphrase = null,
                    burn = true,
                    relay = relay,
                )
            }

            if (injectRetry) armTransientFailure(host)
            val prepared = FileDropTransfer.prepareReceive(link, null) { relay(it) }
            val received = prepared.use { FileDropTransfer.receive(it, context.cacheDir) }
            received.use {
                assertEquals(size.toULong(), it.metadata.size)
                assertEquals(expectedDigest.digest().toList(), sha256(it.file).toList())
            }
        } finally {
            bytes.fill(0)
            FileDropTransfer.wipeAndDelete(source)
        }
    }

    private fun sha256(file: File): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(64 * 1024)
        try {
            FileInputStream(file).use { input ->
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    digest.update(buffer, 0, count)
                }
            }
            return digest.digest()
        } finally {
            buffer.fill(0)
        }
    }

    private fun relay(host: String) = RelayClient("https://$host") { url ->
        URL(url.toString().replaceFirst("https://", "http://"))
            .openConnection() as HttpURLConnection
    }

    private fun armTransientFailure(host: String) {
        val connection = URL("http://$host/__test/fail-next-part")
            .openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "POST"
            check(connection.responseCode in 200..299) { "Could not arm the local relay retry fault" }
        } finally {
            connection.disconnect()
        }
    }

    private companion object {
        const val MIB = 1024L * 1024
    }
}
