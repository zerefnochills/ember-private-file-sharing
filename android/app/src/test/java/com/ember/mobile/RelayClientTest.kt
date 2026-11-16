package com.ember.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Test
import org.json.JSONObject

class RelayClientTest {
    @Test
    fun relayHostBuildsHttpsEndpointsWithoutPathOrFragment() {
        val client = RelayClient.fromRelayHost("relay.ember.example:8443")

        assertEquals("https://relay.ember.example:8443/v1/pow/challenge", client.endpoint("/v1/pow/challenge"))
    }

    @Test
    fun rejectsNonHttpsOrigins() {
        assertThrows(IllegalArgumentException::class.java) { RelayClient("http://relay.ember.example") }
        assertThrows(IllegalArgumentException::class.java) { RelayClient("https://user:secret@relay.ember.example") }
        assertThrows(IllegalArgumentException::class.java) { RelayClient("https://relay.ember.example/api") }
    }

    @Test
    fun relayHostCannotInjectPathOrCredentials() {
        assertThrows(IllegalArgumentException::class.java) { RelayClient.fromRelayHost("relay.example/path") }
        assertThrows(IllegalArgumentException::class.java) { RelayClient.fromRelayHost("user@relay.example") }
        assertThrows(IllegalArgumentException::class.java) { RelayClient.fromRelayHost("relay.example?x=y") }
    }

    @Test
    fun endpointRejectsAuthorityAndRelativePathInjection() {
        val client = RelayClient.fromRelayHost("relay.ember.example")

        assertThrows(IllegalArgumentException::class.java) { client.endpoint("//attacker.example/path") }
        assertThrows(IllegalArgumentException::class.java) { client.endpoint("v1/blobs") }
    }

    @Test
    fun reportSubmitsOnlyExplicitlySelectedOptionalFieldsToConfiguredRelay() {
        val server = FakeRelayServer(burn = true)
        val client = server.client("relay.ember.example")

        val receipt = client.submitReport(
            blobId = "AAAAAAAAAAAAAAAAAAAAAA",
            reason = "malware",
            notes = "I chose to include this note.",
        )

        assertEquals("BBBBBBBBBBBBBBBBBBBBBB", receipt.id)
        assertEquals("submitted", receipt.status)
        val submitted = requireNotNull(server.lastReport)
        assertEquals("/v1/reports", submitted.optString("request_path"))
        val body = submitted.getJSONObject("body")
        assertEquals("AAAAAAAAAAAAAAAAAAAAAA", body.getString("target_id"))
        assertEquals("blob", body.getString("target_type"))
        assertEquals("malware", body.getString("reason"))
        assertEquals("I chose to include this note.", body.getString("notes"))
        assertTrue(body.isNull("content"))
        assertTrue(body.isNull("link_key"))

        client.submitReport(
            blobId = "AAAAAAAAAAAAAAAAAAAAAA",
            reason = "abuse",
            content = "explicit text selection",
            linkKey = "explicit-fragment-selection",
        )
        val secondBody = requireNotNull(server.lastReport).getJSONObject("body")
        assertEquals("explicit text selection", secondBody.getString("content"))
        assertEquals("explicit-fragment-selection", secondBody.getString("link_key"))
    }

    @Test
    fun textCiphertextSplitsAtProtocolPartBoundaries() {
        val ciphertext = ByteArray(4_137 + RelayClient.MAX_PART_BYTES) { (it % 251).toByte() }

        val parts = TextDropTransfer.splitCiphertext(ciphertext)

        assertEquals(listOf(4_137, RelayClient.MAX_PART_BYTES), parts.map(ByteArray::size))
        assertEquals(ciphertext.toList(), (parts[0] + parts[1]).toList())
    }

    @Test
    fun textCiphertextRejectsMissingOrExtraParts() {
        assertThrows(IllegalArgumentException::class.java) {
            TextDropTransfer.splitCiphertext(ByteArray(4_137))
        }
        assertThrows(IllegalArgumentException::class.java) {
            TextDropTransfer.splitCiphertext(ByteArray(4_137 + RelayClient.MAX_PART_BYTES + 1))
        }
    }
}
