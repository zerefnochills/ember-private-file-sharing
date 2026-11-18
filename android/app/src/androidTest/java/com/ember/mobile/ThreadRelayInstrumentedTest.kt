package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.FfiException
import com.ember.mobile.core.isValidThreadHandle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.net.HttpURLConnection
import java.net.URL

/** Runs two independent Android thread endpoints against the actual ember-server router. */
@RunWith(AndroidJUnit4::class)
class ThreadRelayInstrumentedTest {
    @Test
    fun twoSessionsHandshakeExchangeGapRejectReplayAndClose() {
        val host = "10.0.2.2:8080"
        val relay = relay(host)
        val key = ByteArray(32) { 0x42 }
        val initiatorCrypto = try { EmberCrypto.fromKeyBytes(key) } finally { key.fill(0) }
        val responderCrypto = EmberCrypto.fromKeyBytes(ByteArray(32) { 0x42 })
        var rendezvous: com.ember.mobile.core.EmberThreadMailbox? = null
        var initiator: com.ember.mobile.core.EmberThreadInitiator? = null
        var initiatorSession: com.ember.mobile.core.EmberThreadSession? = null
        var responderSession: com.ember.mobile.core.EmberThreadSession? = null
        var initOut: com.ember.mobile.core.EmberThreadMailbox? = null
        var initIn: com.ember.mobile.core.EmberThreadMailbox? = null
        var respOut: com.ember.mobile.core.EmberThreadMailbox? = null
        var respIn: com.ember.mobile.core.EmberThreadMailbox? = null
        try {
            rendezvous = initiatorCrypto.deriveThreadRendezvousMailbox()
            assertValidMailboxExpiry(relay.createThreadMailbox(rendezvous))
            initiator = initiatorCrypto.startThreadInitiator()
            val initMessage = initiator.handshakeMessage()
            val initId = relay.pushThreadMessage(rendezvous, initMessage)
            initMessage.fill(0)

            val receivedInit = relay.pullThreadMessages(rendezvous).single()
            val response = try { responderCrypto.respondToThread(receivedInit.payload.copyOf()) }
                finally { receivedInit.payload.fill(0) }
            responderSession = response.session
            val responderMessage = response.responseMessage.copyOf()
            response.responseMessage.fill(0)
            relay.pushThreadMessage(rendezvous, responderMessage).also { responderMessage.fill(0) }

            val receivedResponse = relay.pullThreadMessages(rendezvous, afterId = initId).single()
            initiatorSession = try { initiator.complete(receivedResponse.payload.copyOf()) }
                finally { receivedResponse.payload.fill(0) }
            initiator.endSession()
            initiator = null

            initOut = initiatorSession.mailboxOut()
            initIn = initiatorSession.mailboxIn()
            respOut = responderSession.mailboxOut()
            respIn = responderSession.mailboxIn()
            assertValidMailboxExpiry(relay.createThreadMailbox(initOut))
            assertValidMailboxExpiry(relay.createThreadMailbox(initIn))

            val deliveredAtoB = mutableListOf<ByteArray>()
            for (text in listOf("A-0", "A-gap", "A-2", "A-3")) {
                val envelope = initiatorSession.encryptText(text)
                if (text != "A-gap") {
                    relay.pushThreadMessage(initOut, envelope)
                    deliveredAtoB += envelope.copyOf()
                }
                envelope.fill(0)
            }
            val incomingAtoB = relay.pullThreadMessages(respIn)
            assertEquals(3, incomingAtoB.size)
            val firstEnvelope = deliveredAtoB.first()
            for ((index, item) in incomingAtoB.withIndex()) {
                val message = try { responderSession.decryptMessage(item.payload.copyOf()) }
                    finally { item.payload.fill(0) }
                assertTrue(isValidThreadHandle(message.senderHandle))
                assertEquals(listOf("A-0", "A-2", "A-3")[index], message.text)
            }
            try {
                responderSession.decryptMessage(firstEnvelope.copyOf())
                error("A replayed sequence was accepted")
            } catch (_: FfiException.ThreadReplay) {
                // Required monotonic sequence rejection.
            } finally {
                firstEnvelope.fill(0)
                deliveredAtoB.forEach { it.fill(0) }
            }

            val deliveredBtoA = mutableListOf<ByteArray>()
            for (index in 0..2) {
                val envelope = responderSession.encryptText("B-$index")
                relay.pushThreadMessage(respOut, envelope)
                deliveredBtoA += envelope.copyOf()
                envelope.fill(0)
            }
            val incomingBtoA = relay.pullThreadMessages(initIn)
            assertEquals(3, incomingBtoA.size)
            for ((index, item) in incomingBtoA.withIndex()) {
                val message = try { initiatorSession.decryptMessage(item.payload.copyOf()) }
                    finally { item.payload.fill(0) }
                assertTrue(isValidThreadHandle(message.senderHandle))
                assertEquals("B-$index", message.text)
            }
            deliveredBtoA.forEach { it.fill(0) }
            assertTrue(relay.getPowChallenge().difficulty <= 32u)
        } finally {
            listOfNotNull(rendezvous, initOut, initIn).distinctBy { it.id() }
                .forEach { runCatching { relay.deleteThreadMailbox(it) }; it.endSession() }
            initiatorSession?.endSession()
            responderSession?.endSession()
            initiator?.endSession()
            initiatorCrypto.close()
            responderCrypto.close()
        }
    }

    private fun relay(host: String) = RelayClient("https://$host") { url ->
        URL(url.toString().replaceFirst("https://", "http://"))
            .openConnection() as HttpURLConnection
    }

    private fun assertValidMailboxExpiry(mailbox: CreatedThreadMailbox) {
        val now = System.currentTimeMillis() / 1_000
        assertTrue(mailbox.expiresAt.toLong() >= now + 43_190)
        assertTrue(mailbox.expiresAt.toLong() <= now + 43_201)
        assertEquals(22, mailbox.id.length)
    }
}
