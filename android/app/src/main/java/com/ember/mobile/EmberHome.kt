package com.ember.mobile

import android.graphics.Bitmap
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import java.util.concurrent.Executors
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter

private enum class EmberPage { Send, Receive }

@androidx.compose.runtime.Composable
fun EmberHome(initialLink: String?, onShareLink: (String) -> Unit) {
    val executor = remember { Executors.newSingleThreadExecutor() }
    val mainHandler = remember { Handler(Looper.getMainLooper()) }
    var busy by remember { mutableStateOf(false) }
    var page by remember { mutableStateOf(EmberPage.Send) }
    var status by remember { mutableStateOf("Private, encrypted text transfer") }

    var relayHost by remember { mutableStateOf("") }
    var sendText by remember { mutableStateOf("") }
    var sendPassphrase by remember { mutableStateOf("") }
    var burnAfterView by remember { mutableStateOf(false) }
    var shareLink by remember { mutableStateOf("") }

    var receiveLink by remember { mutableStateOf(initialLink.orEmpty()) }
    var receivePassphrase by remember { mutableStateOf("") }
    var prepared by remember { mutableStateOf<PreparedTextReceive?>(null) }
    var receivedText by remember { mutableStateOf("") }
    var showBurnConfirmation by remember { mutableStateOf(false) }
    val context = LocalContext.current

    LaunchedEffect(initialLink) {
        if (!initialLink.isNullOrBlank()) {
            prepared?.close()
            prepared = null
            receiveLink = initialLink
            receivedText = ""
            page = EmberPage.Receive
        }
    }

    fun <T> runTask(operation: () -> T, onSuccess: (T) -> Unit = {}) {
        if (busy) return
        busy = true
        status = "Working…"
        executor.execute {
            val result = runCatching(operation)
            mainHandler.post {
                busy = false
                result.fold(
                    onSuccess = { value ->
                        status = "Ready"
                        onSuccess(value)
                    },
                    onFailure = { status = "Transfer failed. Check the relay, link, and passphrase." },
                )
            }
        }
    }

    fun openPreparedDrop() {
        val selected = prepared ?: return
        runTask({ TextDropTransfer.receive(selected) }) { plaintext ->
            receivedText = plaintext
            selected.close()
            if (prepared === selected) prepared = null
            status = "Text received and verified"
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            executor.shutdownNow()
            prepared?.close()
        }
    }

    if (showBurnConfirmation) {
        AlertDialog(
            onDismissRequest = { showBurnConfirmation = false },
            title = { Text("Open this one-view drop?") },
            text = {
                Text("Continuing starts the 10-minute claim window. If this download does not finish in time, the relay deletes the drop.")
            },
            confirmButton = {
                TextButton(onClick = {
                    showBurnConfirmation = false
                    openPreparedDrop()
                }) { Text("Open once") }
            },
            dismissButton = { TextButton(onClick = { showBurnConfirmation = false }) { Text("Cancel") } },
        )
    }

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Ember", style = MaterialTheme.typography.headlineMedium)
        Text("Text stays encrypted until the recipient opens it.", style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = !busy, onClick = { page = EmberPage.Send }) { Text("Send") }
            Button(enabled = !busy, onClick = { page = EmberPage.Receive }) { Text("Receive") }
        }

        if (page == EmberPage.Send) {
            OutlinedTextField(
                value = relayHost,
                onValueChange = { relayHost = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Relay host") },
                placeholder = { Text("relay.example.org") },
                singleLine = true,
                enabled = !busy,
            )
            OutlinedTextField(
                value = sendText,
                onValueChange = { sendText = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Text to send (up to 1 MiB)") },
                minLines = 5,
                enabled = !busy,
            )
            OutlinedTextField(
                value = sendPassphrase,
                onValueChange = { sendPassphrase = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Optional link passphrase") },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
                enabled = !busy,
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = burnAfterView, onCheckedChange = { burnAfterView = it }, enabled = !busy)
                Text("Delete after one completed view")
            }
            Button(
                enabled = !busy && sendText.isNotBlank() && relayHost.isNotBlank(),
                onClick = {
                    val host = relayHost
                    val text = sendText
                    val passphrase = sendPassphrase.takeIf(String::isNotEmpty)
                    val burn = burnAfterView
                    runTask({ TextDropTransfer.send(host, text, passphrase, burn) }) { generatedLink ->
                        shareLink = generatedLink
                        sendText = ""
                        sendPassphrase = ""
                        status = "Encrypted text is ready to share"
                    }
                },
            ) { Text(if (busy) "Sending…" else "Encrypt and send") }

            if (shareLink.isNotEmpty()) {
                Text("Share link", style = MaterialTheme.typography.titleMedium)
                SelectionContainer { Text(shareLink, modifier = Modifier.fillMaxWidth()) }
                LinkQrCode(shareLink)
                Button(enabled = !busy, onClick = { onShareLink(shareLink) }) { Text("Share link…") }
            }
        } else {
            OutlinedTextField(
                value = receiveLink,
                onValueChange = {
                    if (it != receiveLink) {
                        prepared?.close()
                        prepared = null
                    }
                    receiveLink = it
                },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Paste or share an Ember link") },
                minLines = 2,
                enabled = !busy,
            )
            OutlinedTextField(
                value = receivePassphrase,
                onValueChange = { receivePassphrase = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Link passphrase, if required") },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
                enabled = !busy,
            )
            Button(
                enabled = !busy && receiveLink.isNotBlank(),
                onClick = {
                    prepared?.close()
                    prepared = null
                    receivedText = ""
                    val link = receiveLink
                    val passphrase = receivePassphrase.takeIf(String::isNotEmpty)
                    runTask({ TextDropTransfer.prepareReceive(link, passphrase) }) { candidate ->
                        prepared = candidate
                        receivePassphrase = ""
                        status = if (candidate.metadata.burn) {
                            "One-view drop checked. Confirm before opening."
                        } else {
                            "Drop checked. Ready to receive text."
                        }
                    }
                },
            ) { Text("Check link") }

            prepared?.let { candidate ->
                Text("${candidate.metadata.parts} encrypted parts · expires ${candidate.metadata.expiresAt}")
                if (candidate.metadata.burn) {
                    Text("This drop can be opened once. Claiming starts a 10-minute window.")
                }
                Button(enabled = !busy, onClick = {
                    if (candidate.metadata.burn) showBurnConfirmation = true else openPreparedDrop()
                }) { Text(if (busy) "Receiving…" else "Receive text") }
            }
            if (receivedText.isNotEmpty()) {
                Text("Verified text", style = MaterialTheme.typography.titleMedium)
                SelectionContainer { Text(receivedText, modifier = Modifier.fillMaxWidth()) }
            }
        }

        Button(enabled = !busy, onClick = {
            runTask({ SharedVectorRunner.run(context) }) { count -> status = "All $count vector cases passed" }
        }) { Text("Run shared test vectors") }
        Text(status, style = MaterialTheme.typography.bodySmall)
        if (busy) Text("Keep Ember open while the transfer runs.", style = MaterialTheme.typography.bodySmall)
    }
}

@androidx.compose.runtime.Composable
private fun LinkQrCode(link: String) {
    val bitmap = remember(link) {
        runCatching {
            val hints = mapOf(EncodeHintType.MARGIN to 1)
            val matrix = QRCodeWriter().encode(link, BarcodeFormat.QR_CODE, 512, 512, hints)
            Bitmap.createBitmap(matrix.width, matrix.height, Bitmap.Config.ARGB_8888).apply {
                for (y in 0 until matrix.height) {
                    for (x in 0 until matrix.width) {
                        setPixel(x, y, if (matrix[x, y]) 0xFF000000.toInt() else -1)
                    }
                }
            }
        }.getOrNull()
    }
    if (bitmap != null) {
        Image(
            bitmap = bitmap.asImageBitmap(),
            contentDescription = "QR code containing the Ember share link",
            modifier = Modifier.size(240.dp),
            contentScale = ContentScale.Fit,
        )
        HorizontalDivider()
    } else {
        Text("This link is too long to display as a QR code. Share or copy the link instead.")
    }
}

fun shareEmberLink(context: android.content.Context, link: String) {
    val sendIntent = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, link)
    }
    context.startActivity(Intent.createChooser(sendIntent, "Share Ember link"))
}
