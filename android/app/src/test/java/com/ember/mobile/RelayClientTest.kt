package com.ember.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

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
