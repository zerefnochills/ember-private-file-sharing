package com.ember.mobile

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.OpenableColumns
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
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.core.content.ContextCompat
import java.util.concurrent.Executors
import java.io.FileInputStream
import java.io.FileOutputStream
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
    var status by remember { mutableStateOf("Private, encrypted text and file transfer") }

    var relayHost by remember { mutableStateOf("") }
    var sendText by remember { mutableStateOf("") }
    var sendPassphrase by remember { mutableStateOf("") }
    var burnAfterView by remember { mutableStateOf(false) }
    var shareLink by remember { mutableStateOf("") }

    var receiveLink by remember { mutableStateOf(initialLink.orEmpty()) }
    var receivePassphrase by remember { mutableStateOf("") }
    var prepared by remember { mutableStateOf<PreparedTextReceive?>(null) }
    var preparedFile by remember { mutableStateOf<PreparedFileReceive?>(null) }
    var verifiedFile by remember { mutableStateOf<VerifiedFileReceive?>(null) }
    var receivedText by remember { mutableStateOf("") }
    var showBurnConfirmation by remember { mutableStateOf(false) }
    var showScanner by remember { mutableStateOf(false) }
    val context = LocalContext.current
    LaunchedEffect(Unit) {
        executor.execute {
            context.cacheDir.listFiles().orEmpty()
                .filter { it.isFile && it.name.startsWith("ember-verified-") }
                .forEach(FileDropTransfer::wipeAndDelete)
        }
    }
    val cameraPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            showScanner = true
        } else {
            status = "Camera access was not granted. You can paste or share a link instead."
        }
    }
    LaunchedEffect(initialLink) {
        if (!initialLink.isNullOrBlank()) {
            prepared?.close()
            prepared = null
            preparedFile?.close()
            preparedFile = null
            verifiedFile?.close()
            verifiedFile = null
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
                    onFailure = { error -> status = transferErrorMessage(error) },
                )
            }
        }
    }

    val saveFilePicker = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val verified = verifiedFile
        if (uri == null) {
            verified?.close()
            verifiedFile = null
            status = "Save canceled. The verified private copy was deleted."
        } else if (verified != null) {
            runTask({
                val output = context.contentResolver.openOutputStream(uri, "w")
                    ?: throw IllegalStateException("Could not open the selected destination")
                output.use { sink ->
                    FileInputStream(verified.file).use { source ->
                        val buffer = ByteArray(64 * 1024)
                        try {
                            while (true) {
                                val count = source.read(buffer)
                                if (count < 0) break
                                if (count > 0) sink.write(buffer, 0, count)
                            }
                        } finally {
                            buffer.fill(0)
                        }
                    }
                }
                "File saved"
            }) { message ->
                verified.close()
                if (verifiedFile === verified) verifiedFile = null
                status = message
            }
        }
    }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            val host = relayHost.trim()
            val passphrase = sendPassphrase.takeIf(String::isNotEmpty)
            val burn = burnAfterView
            runTask({
                val info = queryPickedFile(context, uri)
                context.contentResolver.openInputStream(uri)?.use { input ->
                    FileDropTransfer.send(
                        relayHost = host,
                        input = input,
                        name = info.name,
                        mime = info.mime,
                        size = info.size,
                        passphrase = passphrase,
                        burn = burn,
                        onProgress = { stage -> mainHandler.post { if (busy) status = stage } },
                    )
                } ?: throw IllegalStateException("Could not open the selected file")
            }) { generatedLink ->
                shareLink = generatedLink
                sendPassphrase = ""
                status = "Encrypted file is ready to share"
            }
        }
    }

    fun openPreparedDrop() {
        val selected = prepared ?: return
        runTask({
            TextDropTransfer.receive(selected) { stage ->
                mainHandler.post { if (busy) status = stage }
            }
        }) { plaintext ->
            receivedText = plaintext
            selected.close()
            if (prepared === selected) prepared = null
            status = "Text received and verified"
        }
    }

    fun openPreparedFile() {
        val selected = preparedFile ?: return
        runTask({
            FileDropTransfer.receive(selected, context.cacheDir) { stage ->
                mainHandler.post { if (busy) status = stage }
            }
        }) { verified ->
            selected.close()
            if (preparedFile === selected) preparedFile = null
            verifiedFile?.close()
            verifiedFile = verified
            status = "File verified. Choose where to save it."
            saveFilePicker.launch(verified.metadata.name.safeSuggestedFileName())
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            executor.shutdownNow()
            prepared?.close()
            preparedFile?.close()
            verifiedFile?.close()
        }
    }

    if (showBurnConfirmation) {
        AlertDialog(
            onDismissRequest = { showBurnConfirmation = false },
            title = { Text("Open this one-view drop?") },
            text = {
                Text("Continuing claims this one-view drop for this device and starts its 10-minute window. If receiving fails or you cancel, the relay may still consume the view.")
            },
            confirmButton = {
                TextButton(onClick = {
                    showBurnConfirmation = false
                    if (preparedFile != null) openPreparedFile() else openPreparedDrop()
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
                    runTask({
                        TextDropTransfer.send(host, text, passphrase, burn, onProgress = { stage ->
                            mainHandler.post { if (busy) status = stage }
                        })
                    }) { generatedLink ->
                        shareLink = generatedLink
                        sendText = ""
                        sendPassphrase = ""
                        status = "Encrypted text is ready to share"
                    }
                },
            ) { Text(if (busy) "Sending…" else "Encrypt and send") }
            Button(
                enabled = !busy && relayHost.isNotBlank(),
                onClick = { filePicker.launch(arrayOf("*/*")) },
            ) { Text("Choose a file to encrypt and send") }

            if (shareLink.isNotEmpty()) {
                Text("Share link", style = MaterialTheme.typography.titleMedium)
                SelectionContainer { Text(shareLink, modifier = Modifier.fillMaxWidth()) }
                LinkQrCode(shareLink)
                Button(enabled = !busy, onClick = { onShareLink(shareLink) }) { Text("Share link…") }
            }
        } else {
            Button(enabled = !busy, onClick = {
                if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
                    showScanner = true
                } else {
                    cameraPermission.launch(Manifest.permission.CAMERA)
                }
            }) { Text("Scan QR code") }
            if (showScanner) {
                QrScanner(
                    onLink = { link ->
                        prepared?.close()
                        prepared = null
                        preparedFile?.close()
                        preparedFile = null
                        verifiedFile?.close()
                        verifiedFile = null
                        receiveLink = link
                        receivedText = ""
                        showScanner = false
                        status = "Ember link scanned. Check it before receiving."
                    },
                    onError = {
                        showScanner = false
                        status = "Camera could not start. Paste or share the link instead."
                    },
                )
                TextButton(onClick = { showScanner = false }) { Text("Close scanner") }
            }
            OutlinedTextField(
                value = receiveLink,
                onValueChange = {
                    if (it != receiveLink) {
                        prepared?.close()
                        prepared = null
                        preparedFile?.close()
                        preparedFile = null
                        verifiedFile?.close()
                        verifiedFile = null
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
                    preparedFile?.close()
                    preparedFile = null
                    receivedText = ""
                    val link = receiveLink
                    val passphrase = receivePassphrase.takeIf(String::isNotEmpty)
                    runTask({ TextDropTransfer.prepareReceive(link, passphrase) }) { candidate ->
                        prepared = candidate
                        preparedFile?.close()
                        preparedFile = null
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
            Button(enabled = !busy && receiveLink.isNotBlank(), onClick = {
                prepared?.close()
                prepared = null
                preparedFile?.close()
                preparedFile = null
                val link = receiveLink
                val passphrase = receivePassphrase.takeIf(String::isNotEmpty)
                runTask({ FileDropTransfer.prepareReceive(link, passphrase) }) { candidate ->
                    preparedFile = candidate
                    receivePassphrase = ""
                    status = if (candidate.metadata.burn) {
                        "One-view drop checked. Confirm before receiving the file."
                    } else {
                        "Drop checked. Ready to receive as a file."
                    }
                }
            }) { Text("Check link for file") }
            preparedFile?.let { candidate ->
                Text("${candidate.metadata.parts} encrypted parts · expires ${candidate.metadata.expiresAt}")
                if (candidate.metadata.burn) Text("Claiming starts the 10-minute one-view window.")
                Button(enabled = !busy, onClick = {
                    if (candidate.metadata.burn) showBurnConfirmation = true else openPreparedFile()
                }) { Text(if (busy) "Receiving…" else "Receive and save file") }
            }
            verifiedFile?.let { verified ->
                Button(enabled = !busy, onClick = {
                    saveFilePicker.launch(verified.metadata.name.safeSuggestedFileName())
                }) { Text("Choose a save location") }
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

private fun transferErrorMessage(error: Throwable): String = when (error) {
    is UnsupportedDropException -> "This drop is a file. Android file receive is waiting for the streaming core API."
    is RelayHttpException -> when (error.status) {
        404, 410 -> "This drop is missing, expired, or already opened."
        409 -> "This one-view drop has already been claimed."
        429 -> "The relay is rate limiting requests. Wait a moment and try again."
        else -> "The relay returned HTTP ${error.status}. Check the link and try again."
    }
    is RelayProtocolException -> "The relay response or encrypted drop could not be verified."
    is IllegalArgumentException -> error.message?.take(160) ?: "Check the relay host and text size."
    else -> "Transfer failed. Check the relay, link, and passphrase."
}

private data class PickedFile(val name: String, val mime: String, val size: Long)

private fun queryPickedFile(context: android.content.Context, uri: Uri): PickedFile {
    var name: String? = null
    var size: Long? = null
    context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) {
            val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
            if (nameColumn >= 0) name = cursor.getString(nameColumn)
            if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) size = cursor.getLong(sizeColumn)
        }
    }
    val safeName = name.orEmpty().substringAfterLast('/').substringAfterLast('\\')
        .filterNot(Char::isISOControl).takeIf(String::isNotBlank) ?: "ember-file.bin"
    val fileSize = size?.takeIf { it > 0 } ?: throw IllegalArgumentException("Choose a non-empty file with a known size")
    return PickedFile(safeName, context.contentResolver.getType(uri) ?: "application/octet-stream", fileSize)
}

private fun String.safeSuggestedFileName(): String = substringAfterLast('/').substringAfterLast('\\')
    .filterNot(Char::isISOControl).takeIf(String::isNotBlank)?.take(180) ?: "ember-file.bin"
