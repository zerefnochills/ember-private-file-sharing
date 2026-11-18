package com.ember.mobile

import android.os.Handler
import android.os.Looper
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.EmberThreadInitiator
import com.ember.mobile.core.EmberThreadMailbox
import com.ember.mobile.core.EmberThreadSession
import com.ember.mobile.core.FfiException
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

enum class ThreadRole { Initiator, Responder }

enum class ThreadProgress {
    RegisteringRendezvous,
    RequestingPowChallenge,
    SolvingPow,
    CreatingMailbox,
    SendingHandshake,
    WaitingForPeer,
    EstablishingSession,
}

data class ThreadUiMessage(
    val sequence: ULong,
    val timestamp: ULong,
    val senderHandle: String,
    val text: String,
    val mine: Boolean,
)

data class ThreadUiState(
    val localHandle: String = "",
    val peerHandle: String = "",
    val messages: List<ThreadUiMessage> = emptyList(),
    val progress: ThreadProgress? = null,
    val failure: ThreadFailure? = null,
    val sessionReady: Boolean = false,
    val closed: Boolean = false,
    val expiryEpochSeconds: ULong? = null,
    val rendezvousMailboxId: String = "",
)

enum class ThreadFailure { Relay, Handshake, Expired, TooLong }

private data class MailboxDeleteRequest(val id: String, val pushToken: String)

/** Owns one non-persistent thread session and polls only while the screen is resumed. */
class ThreadCoordinator(
    private val crypto: EmberCrypto,
    private val ownsCrypto: Boolean,
    private val relay: RelayClient,
    private val role: ThreadRole,
    private val onState: (ThreadUiState) -> Unit,
) {
    private val worker = ScheduledThreadPoolExecutor(1)
    private val cleanupWorker = ScheduledThreadPoolExecutor(1)
    private val main = Handler(Looper.getMainLooper())
    private val isClosed = AtomicBoolean(false)
    @Volatile private var visible = false
    private var started = false
    private var sessionReady = false
    private var state = ThreadUiState()
    private var rendezvous: EmberThreadMailbox? = null
    private var initiator: EmberThreadInitiator? = null
    private var session: EmberThreadSession? = null
    private var outbound: EmberThreadMailbox? = null
    private var inbound: EmberThreadMailbox? = null
    private var initialMessageId: Long? = null
    private var initialMessage: ByteArray? = null
    private var pendingResponderMessage: ByteArray? = null
    private var nextPoll: ScheduledFuture<*>? = null
    private var pollDelaySeconds = POLL_INTERVAL_SECONDS

    fun currentState(): ThreadUiState = state

    fun mailboxIdForReport(): String = state.rendezvousMailboxId

    fun setVisible(isVisible: Boolean) {
        if (isClosed.get()) return
        visible = isVisible
        if (!isVisible) {
            nextPoll?.cancel(true)
            nextPoll = null
            return
        }
        if (!started) {
            started = true
            worker.execute(::initialize)
        } else if (sessionReady) {
            schedulePoll(0)
        }
    }

    fun send(text: String) {
        if (isClosed.get() || !sessionReady) return
        val size = text.toByteArray(Charsets.UTF_8).size
        if (size > MAX_THREAD_TEXT_BYTES) {
            publish(state.copy(failure = ThreadFailure.TooLong))
            return
        }
        worker.execute {
            val activeSession = session ?: return@execute
            var envelope: ByteArray? = null
            try {
                envelope = activeSession.encryptText(text)
                relay.pushThreadMessage(outbound ?: return@execute, envelope)
                val localHandle = activeSession.handle()
                val message = ThreadUiMessage(
                    sequence = (envelope[1].toUByte().toULong() shl 56) or
                        (envelope[2].toUByte().toULong() shl 48) or
                        (envelope[3].toUByte().toULong() shl 40) or
                        (envelope[4].toUByte().toULong() shl 32) or
                        (envelope[5].toUByte().toULong() shl 24) or
                        (envelope[6].toUByte().toULong() shl 16) or
                        (envelope[7].toUByte().toULong() shl 8) or
                        envelope[8].toUByte().toULong(),
                    timestamp = System.currentTimeMillis().toULong() / 1_000u,
                    senderHandle = localHandle,
                    text = text,
                    mine = true,
                )
                state = state.copy(messages = state.messages + message, failure = null)
                publish(state)
            } catch (error: FfiException) {
                val failure = if (error is FfiException.ThreadMessageTooLong) ThreadFailure.TooLong else ThreadFailure.Handshake
                publish(state.copy(failure = failure))
            } catch (_: Exception) {
                publish(state.copy(failure = ThreadFailure.Relay))
            } finally {
                envelope?.fill(0)
            }
        }
    }

    fun close() {
        if (!isClosed.compareAndSet(false, true)) return
        visible = false
        nextPoll?.cancel(true)
        val deleteRequests = listOfNotNull(rendezvous, outbound, inbound)
            .distinctBy { runCatching { it.id() }.getOrNull() }
            .mapNotNull { mailbox ->
                runCatching { MailboxDeleteRequest(mailbox.id(), mailbox.pushToken()) }.getOrNull()
            }

        // Wipe Rust-owned session and capability state immediately; network cleanup is asynchronous.
        session?.endSession()
        initiator?.endSession()
        rendezvous?.endSession()
        outbound?.endSession()
        inbound?.endSession()
        initialMessage?.fill(0)
        initialMessage = null
        pendingResponderMessage?.fill(0)
        pendingResponderMessage = null
        if (ownsCrypto) crypto.close()
        session = null
        initiator = null
        rendezvous = null
        outbound = null
        inbound = null
        sessionReady = false
        publish(ThreadUiState(closed = true))
        worker.shutdownNow()
        cleanupWorker.execute {
            deleteRequests.forEach { request ->
                runCatching { relay.deleteThreadMailbox(request.id, request.pushToken) }
            }
            cleanupWorker.shutdown()
        }
    }

    private fun initialize() {
        if (!visible || isClosed.get()) return
        try {
            publish(state.copy(progress = ThreadProgress.RegisteringRendezvous, failure = null))
            val mailbox = crypto.deriveThreadRendezvousMailbox()
            rendezvous = mailbox
            state = state.copy(rendezvousMailboxId = mailbox.id())
            registerMailbox(mailbox)
            if (!visible || isClosed.get()) return
            when (role) {
                ThreadRole.Initiator -> beginInitiator(mailbox)
                ThreadRole.Responder -> {
                    publish(state.copy(progress = ThreadProgress.WaitingForPeer))
                    schedulePoll(0)
                }
            }
        } catch (error: Exception) {
            fail(error)
        }
    }

    private fun beginInitiator(mailbox: EmberThreadMailbox) {
        publish(state.copy(progress = ThreadProgress.SendingHandshake))
        val handshake = crypto.startThreadInitiator()
        initiator = handshake
        val message = handshake.handshakeMessage()
        initialMessage = message.copyOf()
        initialMessageId = relay.pushThreadMessage(mailbox, message)
        message.fill(0)
        publish(state.copy(progress = ThreadProgress.WaitingForPeer))
        schedulePoll(0)
    }

    private fun registerMailbox(mailbox: EmberThreadMailbox) {
        val created = relay.createThreadMailbox(mailbox, THREAD_TTL_SECONDS) { progress ->
            when (progress) {
                ThreadMailboxProgress.RequestingChallenge -> publish(state.copy(progress = ThreadProgress.RequestingPowChallenge))
                ThreadMailboxProgress.SolvingChallenge -> publish(state.copy(progress = ThreadProgress.SolvingPow))
                ThreadMailboxProgress.CreatingMailbox -> publish(state.copy(progress = ThreadProgress.CreatingMailbox))
            }
        }
        val earliestExpiry = state.expiryEpochSeconds?.let { minOf(it, created.expiresAt) } ?: created.expiresAt
        publish(state.copy(expiryEpochSeconds = earliestExpiry))
    }

    private fun schedulePoll(delaySeconds: Long) {
        if (!visible || isClosed.get() || nextPoll?.isDone == false) return
        nextPoll = worker.schedule(::poll, delaySeconds, TimeUnit.SECONDS)
    }

    private fun poll() {
        nextPoll = null
        if (!visible || isClosed.get()) return
        try {
            if (sessionReady) {
                pollSessionMailbox()
            } else if (session != null) {
                finishHandshakeSetup()
            } else if (role == ThreadRole.Initiator) {
                pollInitiatorResponse()
            } else {
                pollForInitiator()
            }
            pollDelaySeconds = POLL_INTERVAL_SECONDS
        } catch (error: Exception) {
            if (isClosed.get()) return
            val failure = when (error) {
                is FfiException.ThreadClosed -> ThreadFailure.Expired
                else -> ThreadFailure.Relay
            }
            publish(state.copy(failure = failure))
            pollDelaySeconds = (pollDelaySeconds * 2).coerceAtMost(MAX_POLL_BACKOFF_SECONDS)
        }
        if (visible && !isClosed.get()) schedulePoll(pollDelaySeconds)
    }

    private fun pollForInitiator() {
        val mailbox = rendezvous ?: return
        val queued = relay.pullThreadMessages(mailbox)
        try {
            for (item in queued) {
                try {
                    publish(state.copy(progress = ThreadProgress.EstablishingSession))
                    val response = try {
                        crypto.respondToThread(item.payload.copyOf())
                    } catch (_: FfiException) {
                        // Ignore malformed or competing initiators until the first valid one.
                        continue
                    }
                    session = response.session
                    outbound = response.session.mailboxOut()
                    inbound = response.session.mailboxIn()
                    pendingResponderMessage = response.responseMessage.copyOf()
                    response.responseMessage.fill(0)
                    finishHandshakeSetup()
                    return
                } finally {
                    item.payload.fill(0)
                }
            }
        } finally {
            queued.forEach { it.payload.fill(0) }
        }
    }

    private fun pollInitiatorResponse() {
        val mailbox = rendezvous ?: return
        val afterId = initialMessageId ?: return
        val queued = relay.pullThreadMessages(mailbox, afterId)
        try {
            for (item in queued) {
                try {
                    val handshake = initiator ?: return
                    val activeSession = handshake.complete(item.payload.copyOf())
                    initiator = null
                    initialMessage?.fill(0)
                    initialMessage = null
                    session = activeSession
                    outbound = activeSession.mailboxOut()
                    inbound = activeSession.mailboxIn()
                    finishHandshakeSetup()
                    return
                } finally {
                    item.payload.fill(0)
                }
            }
        } finally {
            queued.forEach { it.payload.fill(0) }
        }
    }

    /** Finishes setup on retries without accepting a second initiator or rebuilding the session. */
    private fun finishHandshakeSetup() {
        val activeSession = session ?: return
        if (role == ThreadRole.Responder) {
            val response = pendingResponderMessage
            if (response != null) {
                relay.pushThreadMessage(rendezvous ?: return, response)
                response.fill(0)
                pendingResponderMessage = null
            }
        }
        registerMailbox(outbound ?: return)
        registerMailbox(inbound ?: return)
        ready(activeSession)
    }

    private fun pollSessionMailbox() {
        val mailbox = inbound ?: return
        val activeSession = session ?: return
        val queued = relay.pullThreadMessages(mailbox)
        try {
            for (item in queued) {
                try {
                    val decrypted = activeSession.decryptMessage(item.payload.copyOf())
                    val localHandle = activeSession.handle()
                    val incoming = ThreadUiMessage(
                        sequence = decrypted.sequence,
                        timestamp = decrypted.timestamp,
                        senderHandle = decrypted.senderHandle,
                        text = decrypted.text,
                        mine = decrypted.senderHandle == localHandle,
                    )
                    val messages = (state.messages + incoming).takeLast(MAX_VISIBLE_MESSAGES)
                    val peerHandle = if (incoming.mine) state.peerHandle else incoming.senderHandle
                    publish(state.copy(messages = messages, peerHandle = peerHandle, failure = null))
                } catch (_: FfiException.ThreadReplay) {
                    // Duplicates are discarded after the core's monotonic sequence check.
                } catch (_: FfiException) {
                    publish(state.copy(failure = ThreadFailure.Handshake))
                } finally {
                    item.payload.fill(0)
                }
            }
        } finally {
            queued.forEach { it.payload.fill(0) }
        }
    }

    private fun ready(activeSession: EmberThreadSession) {
        sessionReady = true
        val handle = activeSession.handle()
        publish(state.copy(localHandle = handle, progress = null, sessionReady = true, failure = null))
        schedulePoll(0)
    }

    private fun fail(error: Exception) {
        val failure = if (error is FfiException) ThreadFailure.Handshake else ThreadFailure.Relay
        publish(state.copy(progress = null, failure = failure))
    }

    private fun publish(next: ThreadUiState) {
        state = next
        main.post { if (!isClosed.get() || next.closed) onState(next) }
    }

    companion object {
        const val MAX_THREAD_TEXT_BYTES = 4_096
        private const val THREAD_TTL_SECONDS = 43_200L
        private const val POLL_INTERVAL_SECONDS = 2L
        private const val MAX_POLL_BACKOFF_SECONDS = 30L
        private const val MAX_VISIBLE_MESSAGES = 1_000
    }
}
