package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

@RunWith(AndroidJUnit4::class)
class CliInteropInstrumentedTest {
    @Test
    fun receivesCliFileDropWithPassphraseAndBurn() {
        val args = InstrumentationRegistry.getArguments()
        val link = args.getString("m10_link") ?: return
        val passphrase = args.getString("m10_passphrase") ?: return
        val expectedDigest = args.getString("m10_sha256") ?: return
        val context = InstrumentationRegistry.getInstrumentation().targetContext

        val prepared = FileDropTransfer.prepareReceive(link, passphrase) { relayHost ->
            val emulatorHost = relayHost.replaceFirst("127.0.0.1", "10.0.2.2")
            RelayClient("https://$emulatorHost") { url ->
                URL(url.toString().replaceFirst("https://", "http://"))
                    .openConnection() as HttpURLConnection
            }
        }
        val verified = prepared.use { FileDropTransfer.receive(it, context.cacheDir) }
        verified.use {
            assertTrue(it.metadata.burn)
            assertEquals(expectedDigest, sha256(it.file))
        }
    }

    private fun sha256(file: File): String = file.inputStream().use { input ->
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(64 * 1024)
        try {
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
            digest.digest().joinToString("") { byte -> "%02x".format(byte) }
        } finally {
            buffer.fill(0)
        }
    }
}
