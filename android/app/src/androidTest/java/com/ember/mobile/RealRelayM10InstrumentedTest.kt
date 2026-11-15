package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ember.mobile.core.inspectLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assume.assumeNoException
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.net.URL
import java.security.SecureRandom
import java.security.cert.X509Certificate
import javax.net.ssl.HostnameVerifier
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLContext
import javax.net.ssl.X509TrustManager

/** Optional M10 smoke test; run with the real ember-server on host port 8080 and TLS proxy on 8443. */
@RunWith(AndroidJUnit4::class)
class RealRelayM10InstrumentedTest {
    @get:Rule
    val appRule = androidx.compose.ui.test.junit4.createAndroidComposeRule<MainActivity>()

    @Test
    fun realServerBurnReceiveAndTamperRejection() {
        val host = "10.0.2.2:8443"
        val relay = relay(host)
        try {
            relay.getPowChallenge()
        } catch (error: Exception) {
            assumeNoException("Local real-server TLS proxy is not running", error)
            return
        }

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

    private fun relay(host: String) = RelayClient("https://$host", ::insecureLocalTestConnection)

    private fun armTamperProxy(host: String) {
        val connection = insecureLocalTestConnection(URL("https://$host/__test/tamper-next-part"))
        try {
            connection.requestMethod = "POST"
            check(connection.responseCode in 200..299) { "Local tamper proxy did not arm" }
        } finally {
            connection.disconnect()
        }
    }

    private fun insecureLocalTestConnection(url: URL): HttpsURLConnection =
        (url.openConnection() as HttpsURLConnection).apply {
            sslSocketFactory = TRUST_ALL_TEST_SSL.socketFactory
            hostnameVerifier = HostnameVerifier { _, _ -> true }
        }

    companion object {
        private val TRUST_ALL_TEST_SSL: SSLContext by lazy {
            val trustAll = object : X509TrustManager {
                override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit
                override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit
                override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
            }
            SSLContext.getInstance("TLS").apply {
                init(null, arrayOf(trustAll), SecureRandom())
            }
        }
    }
}
