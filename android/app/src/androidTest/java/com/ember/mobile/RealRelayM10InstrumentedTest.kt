package com.ember.mobile

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ember.mobile.core.inspectLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.net.HttpURLConnection
import java.net.URL

/** Exercises the actual ember-server router; Gradle starts it for connected instrumentation. */
@RunWith(AndroidJUnit4::class)
class RealRelayM10InstrumentedTest {
    @get:Rule
    val appRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun realServerBurnReceiveAndTamperRejection() {
        val host = "10.0.2.2:8080"
        val relay = relay(host)
        assertEquals(0u, relay.getPowChallenge().difficulty)

        val burnLink = TextDropTransfer.sendWithRelay(
            relayHost = host,
            text = "M10 real relay burn check",
            passphrase = null,
            burn = true,
            ttlSeconds = 3_600,
            relay = relay,
        )
        val burnReceive = TextDropTransfer.prepareReceive(burnLink, null) { relay(it) }
        assertEquals(
            "M10 real relay burn check",
            burnReceive.use { TextDropTransfer.receive(it) },
        )

        val tamperLink = TextDropTransfer.sendWithRelay(
            relayHost = host,
            text = "M10 real relay tamper check",
            passphrase = null,
            burn = true,
            ttlSeconds = 3_600,
            relay = relay(host),
        )
        val blobId = inspectLink(tamperLink).blobId
        armTamperProxy(host)
        val tamperReceive = TextDropTransfer.prepareReceive(tamperLink, null) { relay(it) }
        assertThrows(Exception::class.java) {
            tamperReceive.use { TextDropTransfer.receive(it) }
        }

        val afterTamper = relay(host)
        assertEquals("available", afterTamper.getBlob(blobId).state)
        val activeClaim = assertThrows(RelayHttpException::class.java) { afterTamper.claimBlob(blobId) }
        assertEquals(409, activeClaim.status)
    }

    private fun relay(host: String) = RelayClient("https://$host") { url ->
        val localHttpUrl = URL(url.toString().replaceFirst("https://", "http://"))
        localHttpUrl.openConnection() as HttpURLConnection
    }

    private fun armTamperProxy(host: String) {
        val connection = URL("http://$host/__test/tamper-next-part")
            .openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "POST"
            check(connection.responseCode in 200..299) { "The real-server test harness did not arm tampering" }
        } finally {
            connection.disconnect()
        }
    }
}
