package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.io.IOException

@RunWith(AndroidJUnit4::class)
class AndroidToCliInteropInstrumentedTest {
    @Test
    fun sendsPassphraseFileAndBurnTextLinksToCliHarness() {
        val args = InstrumentationRegistry.getArguments()
        val callbackUrl = args.getString("m10_callback") ?: return
        val passphrase = args.getString("m10_passphrase") ?: return
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File.createTempFile("m10-cli-source-", ".bin", context.cacheDir)
        val block = ByteArray(64 * 1024)
        val links = mutableListOf<String>()
        try {
            FileOutputStream(source).use { output ->
                var offset = 0L
                while (offset < FILE_BYTES) {
                    val count = minOf(block.size.toLong(), FILE_BYTES - offset).toInt()
                    for (index in 0 until count) block[index] = ((offset + index) % 239).toByte()
                    output.write(block, 0, count)
                    offset += count
                }
            }

            val host = "10.0.2.2:8080"
            val relay = relay(host)
            val fileLink = FileInputStream(source).use { input ->
                FileDropTransfer.send(
                    relayHost = host,
                    input = input,
                    name = "android-to-cli.bin",
                    mime = "application/octet-stream",
                    size = FILE_BYTES,
                    passphrase = passphrase,
                    burn = true,
                    relay = relay,
                )
            }
            links += fileLink

            val textLink = TextDropTransfer.sendWithRelay(
                relayHost = host,
                text = CLI_TEXT,
                passphrase = null,
                burn = true,
                ttlSeconds = 3_600,
                relay = relay,
            )
            links += textLink

            val webLink = TextDropTransfer.sendWithRelay(
                relayHost = host,
                text = WEB_TEXT,
                passphrase = null,
                burn = false,
                ttlSeconds = 3_600,
                relay = relay,
            )
            links += webLink
            postLinks(callbackUrl, links)
        } finally {
            block.fill(0)
            FileDropTransfer.wipeAndDelete(source)
        }
    }

    private fun postLinks(callbackUrl: String, links: List<String>) {
        val bytes = links.joinToString("\n").toByteArray(Charsets.UTF_8)
        var lastError: IOException? = null
        try {
            repeat(3) { attempt ->
                val connection = URL(callbackUrl).openConnection() as HttpURLConnection
                try {
                    connection.requestMethod = "POST"
                    connection.connectTimeout = 5_000
                    connection.readTimeout = 5_000
                    connection.doOutput = true
                    connection.setRequestProperty("Content-Type", "text/plain; charset=utf-8")
                    connection.setFixedLengthStreamingMode(bytes.size)
                    connection.outputStream.use { it.write(bytes) }
                    if (connection.responseCode in 200..299) return
                    throw IOException("M10 local link collector rejected a test result")
                } catch (error: IOException) {
                    lastError = error
                    if (attempt < 2) Thread.sleep(100L shl attempt)
                } finally {
                    connection.disconnect()
                }
            }
        } finally {
            bytes.fill(0)
        }
        throw lastError ?: IOException("M10 local link collector did not accept the test result")
    }

    private fun relay(host: String) = RelayClient("https://$host") { url ->
        URL(url.toString().replaceFirst("https://", "http://"))
            .openConnection() as HttpURLConnection
    }

    private companion object {
        const val FILE_BYTES = 256L * 1024
        const val CLI_TEXT = "Android-to-CLI M10 text interoperability"
        const val WEB_TEXT = "Android link opened and decrypted by the web receiver"
    }
}
