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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import java.util.concurrent.Executors
import java.io.FileInputStream
import java.io.FileOutputStream
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.ember.mobile.core.inspectLink

private enum class EmberPage { Send, Receive }

@androidx.compose.runtime.Composable
fun EmberHome(initialLink: String?, onShareLink: (String) -> Unit) {
    val context = LocalContext.current
    val executor = remember { Executors.newSingleThreadExecutor() }
    val mainHandler = remember { Handler(Looper.getMainLooper()) }
    var busy by remember { mutableStateOf(false) }
    var page by remember { mutableStateOf(EmberPage.Send) }
    var status by remember { mutableStateOf(context.getString(R.string.status_idle)) }

    var relayHost by remember { mutableStateOf("") }
    var sendText by remember { mutableStateOf("") }
    var sendPassphrase by remember { mutableStateOf("") }
    var burnAfterView by remember { mutableStateOf(false) }
    var shareLink by remember { mutableStateOf("") }

    var receiveLink by remember { mutableStateOf(initialLink.orEmpty()) }
    var receivePassphrase by remember { mutableStateOf("") }
    var prepared by remember { mutableStateOf<PreparedTextReceive?>(null) }
    var preparedFile by remember { mutableStateOf<PreparedFileReceive?>(null) }
    var pendingSendFile by remember { mutableStateOf<PreparedFileSend?>(null) }
    var verifiedFile by remember { mutableStateOf<VerifiedFileReceive?>(null) }
    var receivedText by remember { mutableStateOf("") }
    var showBurnConfirmation by remember { mutableStateOf(false) }
    var showScanner by remember { mutableStateOf(false) }
    var blockedDropKeys by remember(context) { mutableStateOf(DropBlocklist.read(context)) }
    val currentDropInfo = remember(receiveLink) {
        runCatching { inspectLink(receiveLink.trim()) }.getOrNull()
    }
    val currentBlockKey = currentDropInfo?.let { DropBlocklist.key(it.host, it.blobId) }
    val currentDropBlocked = currentBlockKey != null && currentBlockKey in blockedDropKeys
    var showReportDialog by remember { mutableStateOf(false) }
    var showBlockConfirmation by remember { mutableStateOf(false) }
    var reportReason by remember(context) { mutableStateOf(context.getString(R.string.default_report_reason)) }
    var reportNotes by remember { mutableStateOf("") }
    var includeReportText by remember { mutableStateOf(false) }
    var includeReportKey by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        executor.execute {
            context.cacheDir.listFiles().orEmpty()
                .filter { it.isFile && (it.name.startsWith("ember-verified-") || it.name.startsWith("ember-send-")) }
                .forEach(FileDropTransfer::wipeAndDelete)
        }
    }
    val cameraPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            showScanner = true
        } else {
            status = context.getString(R.string.camera_permission_denied)
        }
    }
    LaunchedEffect(initialLink) {
        if (initialLink != null) {
            showReportDialog = false
            includeReportText = false
            includeReportKey = false
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
        status = context.getString(R.string.status_working)
        executor.execute {
            val result = runCatching(operation)
            mainHandler.post {
                busy = false
                result.fold(
                    onSuccess = { value ->
                        status = context.getString(R.string.status_ready)
                        onSuccess(value)
                    },
                    onFailure = { error -> status = transferErrorMessage(context, error) },
                )
            }
        }
    }

    val saveFilePicker = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val verified = verifiedFile
        if (uri == null) {
            verified?.close()
            verifiedFile = null
            status = context.getString(R.string.status_save_canceled)
        } else if (verified != null) {
            runTask({
                val output = context.contentResolver.openOutputStream(uri, "w")
                    ?: throw IllegalStateException(context.getString(R.string.error_open_destination))
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
                context.getString(R.string.status_file_saved)
            }) { message ->
                verified.close()
                if (verifiedFile === verified) verifiedFile = null
                status = message
            }
        }
    }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            runTask({
                val info = queryPickedFile(context, uri)
                context.contentResolver.openInputStream(uri)?.use { input ->
                    FileDropTransfer.prepareForSend(
                        input = input,
                        name = info.name,
                        mime = info.mime,
                        size = info.size,
                        cacheDir = context.cacheDir,
                    )
                } ?: throw IllegalStateException(context.getString(R.string.error_open_selected_file))
            }) { candidate ->
                pendingSendFile?.close()
                pendingSendFile = candidate
                status = context.getString(R.string.status_file_prepared)
            }
        }
    }

    fun openPreparedDrop() {
        val selected = prepared ?: return
        runTask({
            TextDropTransfer.receive(selected) { stage ->
                mainHandler.post { if (busy) status = localizedProgress(context, stage) }
            }
        }) { plaintext ->
            receivedText = plaintext
            selected.close()
            if (prepared === selected) prepared = null
            status = context.getString(R.string.status_text_received)
        }
    }

    fun openPreparedFile() {
        val selected = preparedFile ?: return
        runTask({
            FileDropTransfer.receive(selected, context.cacheDir) { stage ->
                mainHandler.post { if (busy) status = localizedProgress(context, stage) }
            }
        }) { verified ->
            selected.close()
            if (preparedFile === selected) preparedFile = null
            verifiedFile?.close()
            verifiedFile = verified
            status = context.getString(R.string.status_file_verified)
            saveFilePicker.launch(verified.metadata.name.safeSuggestedFileName())
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            executor.shutdownNow()
            prepared?.close()
            preparedFile?.close()
            verifiedFile?.close()
            pendingSendFile?.close()
        }
    }

    if (showBurnConfirmation) {
        AlertDialog(
            onDismissRequest = { showBurnConfirmation = false },
            title = { Text(stringResource(R.string.burn_dialog_title)) },
            text = {
                Text(stringResource(R.string.burn_dialog_message))
            },
            confirmButton = {
                TextButton(onClick = {
                    showBurnConfirmation = false
                    if (preparedFile != null) openPreparedFile() else openPreparedDrop()
                }) { Text(stringResource(R.string.action_open_once)) }
            },
            dismissButton = { TextButton(onClick = { showBurnConfirmation = false }) { Text(stringResource(R.string.action_cancel)) } },
        )
    }

    if (showReportDialog) {
        AlertDialog(
            onDismissRequest = { showReportDialog = false },
            title = { Text(stringResource(R.string.report_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.report_fields_notice))
                    OutlinedTextField(
                        value = reportReason,
                        onValueChange = { reportReason = it.take(80) },
                        label = { Text(stringResource(R.string.report_reason)) },
                        singleLine = true,
                    )
                    OutlinedTextField(
                        value = reportNotes,
                        onValueChange = { reportNotes = it.take(2_000) },
                        label = { Text(stringResource(R.string.report_note)) },
                        minLines = 2,
                        maxLines = 4,
                    )
                    if (receivedText.isNotBlank()) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(
                                checked = includeReportText,
                                onCheckedChange = { includeReportText = it },
                                modifier = Modifier.semantics {
                                    contentDescription = context.getString(R.string.report_include_text_accessibility, RelayClient.MAX_REPORT_TEXT_CHARS)
                                },
                            )
                            Text(stringResource(R.string.report_include_text, RelayClient.MAX_REPORT_TEXT_CHARS))
                        }
                    }
                    val canIncludeKey = currentDropInfo != null && !currentDropInfo.passphraseProtected &&
                        receiveLink.substringAfter('#', "").isNotBlank()
                    if (canIncludeKey) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(
                                checked = includeReportKey,
                                onCheckedChange = { includeReportKey = it },
                                modifier = Modifier.semantics {
                                    contentDescription = context.getString(R.string.report_include_key_accessibility)
                                },
                            )
                            Text(stringResource(R.string.report_include_key))
                        }
                    } else if (currentDropInfo?.passphraseProtected == true) {
                        Text(stringResource(R.string.report_protected_link))
                    }
                }
            },
            confirmButton = {
                TextButton(
                    enabled = !busy && reportReason.isNotBlank(),
                    onClick = {
                        val info = currentDropInfo ?: return@TextButton
                        val note = reportNotes.trim().takeIf(String::isNotEmpty)
                        val content = receivedText.take(RelayClient.MAX_REPORT_TEXT_CHARS)
                            .takeIf { includeReportText }
                        val linkKey = receiveLink.substringAfter('#', "")
                            .takeIf { includeReportKey && !info.passphraseProtected && it.isNotBlank() }
                        showReportDialog = false
                        runTask({
                            RelayClient.fromRelayHost(info.host).submitReport(
                                blobId = info.blobId,
                                reason = reportReason,
                                notes = note,
                                content = content,
                                linkKey = linkKey,
                            )
                        }) { receipt ->
                            status = context.getString(R.string.status_report_sent_details, receipt.id)
                            reportNotes = ""
                            includeReportText = false
                            includeReportKey = false
                        }
                    },
                ) { Text(stringResource(R.string.action_send_report)) }
            },
            dismissButton = { TextButton(onClick = { showReportDialog = false }) { Text(stringResource(R.string.action_cancel)) } },
        )
    }

    if (showBlockConfirmation) {
        AlertDialog(
            onDismissRequest = { showBlockConfirmation = false },
            title = { Text(stringResource(R.string.block_title)) },
            text = { Text(stringResource(R.string.block_message)) },
            confirmButton = {
                TextButton(onClick = {
                    val key = currentBlockKey ?: return@TextButton
                    blockedDropKeys = DropBlocklist.update(context, blockedDropKeys, key, blocked = true)
                    prepared?.close()
                    prepared = null
                    preparedFile?.close()
                    preparedFile = null
                    verifiedFile?.close()
                    verifiedFile = null
                    receivedText = ""
                    receivePassphrase = ""
                    showBlockConfirmation = false
                    status = context.getString(R.string.status_drop_blocked)
                }) { Text(stringResource(R.string.action_block_drop)) }
            },
            dismissButton = { TextButton(onClick = { showBlockConfirmation = false }) { Text(stringResource(R.string.action_cancel)) } },
        )
    }

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.tagline), style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = !busy, onClick = { page = EmberPage.Send }) { Text(stringResource(R.string.tab_send)) }
            Button(enabled = !busy, onClick = { page = EmberPage.Receive }) { Text(stringResource(R.string.tab_receive)) }
        }

        if (page == EmberPage.Send) {
            OutlinedTextField(
                value = relayHost,
                onValueChange = { relayHost = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.field_relay_host)) },
                placeholder = { Text(stringResource(R.string.relay_host_example)) },
                singleLine = true,
                enabled = !busy,
            )
            OutlinedTextField(
                value = sendText,
                onValueChange = { sendText = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.field_send_text)) },
                minLines = 5,
                enabled = !busy,
            )
            OutlinedTextField(
                value = sendPassphrase,
                onValueChange = { sendPassphrase = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.field_send_passphrase)) },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
                enabled = !busy,
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = burnAfterView, onCheckedChange = { burnAfterView = it }, enabled = !busy)
                Text(stringResource(R.string.burn_after_view))
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
                            mainHandler.post { if (busy) status = localizedProgress(context, stage) }
                        })
                    }) { generatedLink ->
                        shareLink = generatedLink
                        sendText = ""
                        sendPassphrase = ""
                        status = context.getString(R.string.status_text_ready)
                    }
                },
            ) { Text(stringResource(if (busy) R.string.action_sending else R.string.action_encrypt_send)) }
            Button(
                enabled = !busy && relayHost.isNotBlank(),
                onClick = {
                    pendingSendFile?.close()
                    pendingSendFile = null
                    filePicker.launch(arrayOf("*/*"))
                },
            ) { Text(stringResource(R.string.action_choose_file)) }
            pendingSendFile?.let { selected ->
                Text(stringResource(R.string.selected_file, selected.name.safeSuggestedFileName(), selected.size))
                Text(metadataSummaryMessage(context, selected.summary))
                Button(
                    enabled = !busy && relayHost.isNotBlank(),
                    onClick = {
                        val host = relayHost.trim()
                        val passphrase = sendPassphrase.takeIf(String::isNotEmpty)
                        val burn = burnAfterView
                        runTask({
                            FileInputStream(selected.file).use { input ->
                                FileDropTransfer.send(
                                    relayHost = host,
                                    input = input,
                                    name = selected.name,
                                    mime = selected.mime,
                                    size = selected.size,
                                    passphrase = passphrase,
                                    burn = burn,
                                    onProgress = { stage -> mainHandler.post { if (busy) status = localizedProgress(context, stage) } },
                                )
                            }
                        }) { generatedLink ->
                            selected.close()
                            if (pendingSendFile === selected) pendingSendFile = null
                            shareLink = generatedLink
                            sendPassphrase = ""
                            status = context.getString(R.string.status_file_ready)
                        }
                    },
                ) {
                    Text(
                        when {
                            busy -> context.getString(R.string.action_sending_file)
                            selected.summary.requiresExplicitPassThrough -> context.getString(R.string.action_send_without_stripping)
                            else -> context.getString(R.string.action_send_file)
                        },
                    )
                }
                TextButton(enabled = !busy, onClick = {
                    selected.close()
                    if (pendingSendFile === selected) pendingSendFile = null
                    status = context.getString(R.string.status_file_deleted)
                }) { Text(stringResource(R.string.action_cancel_selected_file)) }
            }

            if (shareLink.isNotEmpty()) {
                Text(stringResource(R.string.label_share_link), style = MaterialTheme.typography.titleMedium)
                SelectionContainer { Text(shareLink, modifier = Modifier.fillMaxWidth()) }
                LinkQrCode(shareLink)
                Button(enabled = !busy, onClick = { onShareLink(shareLink) }) { Text(stringResource(R.string.action_share_link)) }
            }
        } else {
            Button(enabled = !busy, onClick = {
                if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
                    showScanner = true
                } else {
                    cameraPermission.launch(Manifest.permission.CAMERA)
                }
            }) { Text(stringResource(R.string.action_scan_qr)) }
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
                        status = context.getString(R.string.status_link_scanned)
                    },
                    onError = {
                        showScanner = false
                        status = context.getString(R.string.status_camera_error)
                    },
                )
                TextButton(onClick = { showScanner = false }) { Text(stringResource(R.string.action_close_scanner)) }
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
                        receivedText = ""
                        showReportDialog = false
                        includeReportText = false
                        includeReportKey = false
                    }
                    receiveLink = it
                },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.field_receive_link)) },
                minLines = 2,
                enabled = !busy,
            )
            if (currentDropBlocked) {
                Text(stringResource(R.string.blocked_drop_notice))
                TextButton(enabled = !busy, onClick = {
                    blockedDropKeys = DropBlocklist.update(context, blockedDropKeys, currentBlockKey, blocked = false)
                    status = context.getString(R.string.status_drop_unblocked)
                }) { Text(stringResource(R.string.action_unblock_drop)) }
            }
            OutlinedTextField(
                value = receivePassphrase,
                onValueChange = { receivePassphrase = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.field_receive_passphrase)) },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
                enabled = !busy,
            )
            Button(
                enabled = !busy && receiveLink.isNotBlank() && !currentDropBlocked,
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
                            context.getString(R.string.status_one_view_ready_text)
                        } else {
                            context.getString(R.string.status_ready_text)
                        }
                    }
                },
            ) { Text(stringResource(R.string.action_check_link)) }

            prepared?.let { candidate ->
                Text(stringResource(R.string.parts_expiry, candidate.metadata.parts, candidate.metadata.expiresAt))
                if (candidate.metadata.burn) {
                    Text(stringResource(R.string.burn_drop_warning))
                }
                Button(enabled = !busy, onClick = {
                    if (candidate.metadata.burn) showBurnConfirmation = true else openPreparedDrop()
                }) { Text(stringResource(if (busy) R.string.action_receiving else R.string.action_receive_text)) }
            }
            Button(enabled = !busy && receiveLink.isNotBlank() && !currentDropBlocked, onClick = {
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
                        context.getString(R.string.status_one_view_ready_file)
                    } else {
                        context.getString(R.string.status_ready_file)
                    }
                }
            }) { Text(stringResource(R.string.action_check_file_link)) }
            preparedFile?.let { candidate ->
                Text(stringResource(R.string.parts_expiry, candidate.metadata.parts, candidate.metadata.expiresAt))
                if (candidate.metadata.burn) Text(stringResource(R.string.burn_file_warning))
                Button(enabled = !busy, onClick = {
                    if (candidate.metadata.burn) showBurnConfirmation = true else openPreparedFile()
                }) { Text(stringResource(if (busy) R.string.action_receiving else R.string.action_receive_save_file)) }
            }
            verifiedFile?.let { verified ->
                Button(enabled = !busy, onClick = {
                    saveFilePicker.launch(verified.metadata.name.safeSuggestedFileName())
                }) { Text(stringResource(R.string.action_choose_save_location)) }
            }
            if (receivedText.isNotEmpty()) {
                Text(stringResource(R.string.label_verified_text), style = MaterialTheme.typography.titleMedium)
                SelectionContainer { Text(receivedText, modifier = Modifier.fillMaxWidth()) }
            }
            if (currentDropInfo != null && (prepared != null || preparedFile != null ||
                    verifiedFile != null || receivedText.isNotEmpty())
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(enabled = !busy, onClick = {
                        reportReason = context.getString(R.string.default_report_reason)
                        reportNotes = ""
                        includeReportText = false
                        includeReportKey = false
                        showReportDialog = true
                    }) { Text(stringResource(R.string.action_report_drop)) }
                    Button(enabled = !busy, onClick = { showBlockConfirmation = true }) { Text(stringResource(R.string.action_block_drop)) }
                }
            }
        }

        Button(enabled = !busy, onClick = {
            runTask({ SharedVectorRunner.run(context) }) { count -> status = context.getString(R.string.status_vectors_passed, count) }
        }) { Text(stringResource(R.string.action_run_test_vectors)) }
        Text(status, style = MaterialTheme.typography.bodySmall)
        if (busy) Text(stringResource(R.string.status_keep_open), style = MaterialTheme.typography.bodySmall)
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
            contentDescription = stringResource(R.string.qr_link_description),
            modifier = Modifier.size(240.dp),
            contentScale = ContentScale.Fit,
        )
        HorizontalDivider()
    } else {
        Text(stringResource(R.string.qr_too_long))
    }
}

fun shareEmberLink(context: android.content.Context, link: String) {
    val sendIntent = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, link)
    }
    context.startActivity(Intent.createChooser(sendIntent, context.getString(R.string.chooser_share_link)))
}

private fun transferErrorMessage(context: android.content.Context, error: Throwable): String = when (error) {
    is UnsupportedDropException -> context.getString(R.string.error_file_not_supported)
    is RelayHttpException -> when (error.status) {
        404, 410 -> context.getString(R.string.error_missing_drop)
        409 -> context.getString(R.string.error_already_claimed)
        429 -> context.getString(R.string.error_rate_limited)
        else -> context.getString(R.string.error_http, error.status)
    }
    is RelayProtocolException -> context.getString(R.string.error_integrity)
    is IllegalArgumentException -> context.getString(R.string.error_check_inputs)
    else -> context.getString(R.string.error_generic)
}

private fun localizedProgress(context: android.content.Context, stage: String): String {
    val indexed = Regex("(Uploading final|Uploading|Downloading|Encrypting and uploading|Downloading and verifying) part (\\d+) of (\\d+)…").matchEntire(stage)
    if (indexed != null) {
        val part = indexed.groupValues[2].toInt()
        val total = indexed.groupValues[3].toInt()
        val resource = when (indexed.groupValues[1]) {
            "Uploading final" -> R.string.progress_upload_final_part
            "Uploading" -> R.string.progress_upload_part
            "Downloading" -> R.string.progress_download_part
            "Encrypting and uploading" -> R.string.progress_encrypt_upload_part
            else -> R.string.progress_download_verify_part
        }
        return context.getString(resource, part, total)
    }
    val resource = when (stage) {
        "Requesting proof-of-work challenge…" -> R.string.progress_request_pow
        "Solving proof of work…" -> R.string.progress_solve_pow
        "Creating encrypted drop…" -> R.string.progress_create_text_drop
        "Creating encrypted file drop…" -> R.string.progress_create_file_drop
        "Finalizing upload…" -> R.string.progress_finalize_upload
        "Finalizing encrypted upload…" -> R.string.progress_finalize_file_upload
        "Building secure share link…" -> R.string.progress_build_link
        "Resuming claimed download…" -> R.string.progress_resume_text
        "Resuming claimed file download…" -> R.string.progress_resume_file
        "Claiming one-view drop…" -> R.string.progress_claim_text
        "Claiming one-view file…" -> R.string.progress_claim_file
        "Starting download…" -> R.string.progress_start_download
        "Starting file download…" -> R.string.progress_start_file_download
        "Authenticating encrypted text…" -> R.string.progress_authenticate_text
        "Authenticating final file chunk…" -> R.string.progress_authenticate_file
        "Finishing verified download…" -> R.string.progress_finish_download
        else -> R.string.status_working
    }
    return context.getString(resource)
}

private fun metadataSummaryMessage(
    context: android.content.Context,
    summary: MetadataStripSummary,
): String = when (summary.status) {
    "stripped" -> context.getString(
        R.string.metadata_removed,
        summary.format,
        summary.fieldsRemoved.joinToString(),
    )
    "clean" -> context.getString(R.string.metadata_clean, summary.format)
    "skipped" -> context.getString(R.string.metadata_skipped)
    else -> context.getString(R.string.metadata_not_stripped, summary.format)
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
