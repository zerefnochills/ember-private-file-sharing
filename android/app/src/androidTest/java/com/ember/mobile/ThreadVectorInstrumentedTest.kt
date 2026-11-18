package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.isValidThreadHandshakeMessage
import com.ember.mobile.core.parseThreadEnvelope
import com.ember.mobile.core.serializeThreadEnvelope
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ThreadVectorInstrumentedTest {
    @Test
    fun threadHandshakeAndV3EnvelopeVectorsPassThroughRustBindings() {
        val root = InstrumentationRegistry.getInstrumentation().targetContext.assets
            .open("thread-v1.json").bufferedReader().use { JSONObject(it.readText()) }
        val vector = root.getJSONArray("handshake_cases").getJSONObject(0)
        val key = hexToBytes(vector.getString("drop_key_hex"))
        val expectedPsk = hexToBytes(vector.getString("psk_hex"))
        val crypto = try {
            EmberCrypto.fromKeyBytes(key)
        } finally {
            key.fill(0)
        }
        try {
            try {
                assertTrue(crypto.matchesThreadPsk(expectedPsk))
            } finally {
                expectedPsk.fill(0)
            }

            val mailbox = crypto.deriveThreadRendezvousMailbox()
            try {
                val expectedMailbox = vector.getJSONObject("rendezvous_mailbox")
                assertEquals(expectedMailbox.getString("id"), mailbox.id())
                assertEquals(expectedMailbox.getString("push_token"), mailbox.pushToken())
                assertEquals(expectedMailbox.getString("pull_token"), mailbox.pullToken())
            } finally {
                mailbox.endSession()
            }

            val initMessage = hexToBytes(vector.getString("initiator_handshake_msg_hex"))
            assertTrue(isValidThreadHandshakeMessage(initMessage.copyOf()))
            val response = crypto.respondToThread(initMessage)
            try {
                assertTrue(isValidThreadHandshakeMessage(response.responseMessage.copyOf()))
                val initOut = response.session.mailboxOut()
                val respIn = response.session.mailboxIn()
                try {
                    assertTrue(initOut.id().matches(Regex("^[A-Za-z0-9_-]{22}$")))
                    assertTrue(respIn.id().matches(Regex("^[A-Za-z0-9_-]{22}$")))
                    assertTrue(initOut.id() != respIn.id())
                } finally {
                    initOut.endSession()
                    respIn.endSession()
                }
            } finally {
                response.responseMessage.fill(0)
                response.session.endSession()
            }

            val messages = vector.getJSONArray("messages")
            for (index in 0 until messages.length()) {
                val item = messages.getJSONObject(index)
                val ciphertext = hexToBytes(item.getString("ciphertext_hex"))
                val envelope = serializeThreadEnvelope(
                    item.getInt("v").toUByte(), item.getLong("seq").toULong(), ciphertext.copyOf(),
                )
                try {
                    val parsed = parseThreadEnvelope(envelope.copyOf())
                    try {
                        assertEquals(item.getInt("v").toUByte(), parsed.version)
                        assertEquals(item.getLong("seq").toULong(), parsed.sequence)
                        assertEquals(ciphertext.toList(), parsed.ciphertext.toList())
                    } finally {
                        parsed.ciphertext.fill(0)
                    }
                    assertEquals(ciphertext.toList(), envelope.copyOfRange(9, envelope.size).toList())
                } finally {
                    ciphertext.fill(0)
                    envelope.fill(0)
                }
            }
        } finally {
            crypto.close()
        }
    }

    private fun hexToBytes(value: String): ByteArray {
        require(value.length % 2 == 0)
        return ByteArray(value.length / 2) { i -> value.substring(i * 2, i * 2 + 2).toInt(16).toByte() }
    }
}
