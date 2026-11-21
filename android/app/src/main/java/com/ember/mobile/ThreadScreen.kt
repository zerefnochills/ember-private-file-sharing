package com.ember.mobile

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

@Composable
fun ThreadScreen(
    state: ThreadUiState,
    onSend: (String) -> Unit,
    onClose: () -> Unit,
    onReport: () -> Unit,
    onBlock: () -> Unit,
) {
    var draft by remember { mutableStateOf("") }
    var nowSeconds by remember { mutableLongStateOf(System.currentTimeMillis() / 1_000) }
    LaunchedEffect(state.expiryEpochSeconds) {
        while (state.expiryEpochSeconds != null) {
            nowSeconds = System.currentTimeMillis() / 1_000
            if (state.expiryEpochSeconds <= nowSeconds.toULong()) {
                onClose()
                break
            }
            delay(1_000)
        }
    }
    val byteCount = draft.toByteArray(Charsets.UTF_8).size
    val expiry = state.expiryEpochSeconds?.let { expiry ->
        val remaining = (expiry.toLong() - nowSeconds).coerceAtLeast(0)
        val hours = remaining / 3_600
        val minutes = remaining % 3_600 / 60
        stringResource(R.string.thread_expiry_timer, hours, minutes)
    } ?: stringResource(R.string.thread_expiry_unavailable)

    Column(
        modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.thread_title), style = MaterialTheme.typography.headlineSmall)
        if (state.localHandle.isNotBlank()) {
            Text(stringResource(R.string.thread_local_handle, state.localHandle))
        }
        Text(
            stringResource(
                R.string.thread_peer_handle,
                state.peerHandle.ifBlank { stringResource(R.string.thread_waiting_handle) },
            ),
        )
        Text(expiry, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        Text(stringResource(R.string.thread_memory_note), style = MaterialTheme.typography.bodySmall)
        Text(stringResource(R.string.thread_relay_note), style = MaterialTheme.typography.bodySmall)

        state.progress?.let { progress ->
            Text(progressText(progress), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        }
        state.failure?.let { failure ->
            Text(failureText(failure), color = MaterialTheme.colorScheme.error)
        }
        if (state.messages.isEmpty()) {
            Text(stringResource(R.string.thread_empty))
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                state.messages.forEach { message ->
                    Column(modifier = Modifier.fillMaxWidth().padding(start = if (message.mine) 32.dp else 0.dp)) {
                        Text(message.senderHandle, style = MaterialTheme.typography.labelMedium)
                        Text(message.text, style = MaterialTheme.typography.bodyLarge)
                    }
                }
            }
        }

        OutlinedTextField(
            value = draft,
            onValueChange = { draft = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text(stringResource(R.string.thread_message_field)) },
            minLines = 2,
            maxLines = 6,
            enabled = state.sessionReady,
        )
        Text(stringResource(R.string.thread_message_counter, byteCount, ThreadCoordinator.MAX_THREAD_TEXT_BYTES))
        Button(
            modifier = Modifier.heightIn(min = 48.dp),
            enabled = state.sessionReady && draft.isNotBlank() && byteCount <= ThreadCoordinator.MAX_THREAD_TEXT_BYTES,
            onClick = {
                onSend(draft)
                draft = ""
            },
        ) { Text(stringResource(R.string.thread_send)) }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onClose) {
                Text(stringResource(R.string.thread_close))
            }
            TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onReport) {
                Text(stringResource(R.string.action_report_drop))
            }
            TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onBlock) {
                Text(stringResource(R.string.thread_block))
            }
        }
    }
}

@Composable
private fun progressText(progress: ThreadProgress): String = when (progress) {
    ThreadProgress.RegisteringRendezvous -> stringResource(R.string.thread_progress_rendezvous)
    ThreadProgress.RequestingPowChallenge -> stringResource(R.string.thread_progress_pow_request)
    ThreadProgress.SolvingPow -> stringResource(R.string.thread_progress_pow_solve)
    ThreadProgress.CreatingMailbox -> stringResource(R.string.thread_progress_mailbox)
    ThreadProgress.SendingHandshake -> stringResource(R.string.thread_progress_handshake)
    ThreadProgress.WaitingForPeer -> stringResource(R.string.thread_progress_waiting)
    ThreadProgress.EstablishingSession -> stringResource(R.string.thread_progress_connecting)
}

@Composable
private fun failureText(failure: ThreadFailure): String = when (failure) {
    ThreadFailure.Relay -> stringResource(R.string.thread_error_relay)
    ThreadFailure.Handshake -> stringResource(R.string.thread_error_handshake)
    ThreadFailure.Expired -> stringResource(R.string.thread_error_expired)
    ThreadFailure.TooLong -> stringResource(R.string.thread_error_too_long)
}
