package com.ember.mobile

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

internal data class PendingReport(
    val host: String,
    val targetId: String,
    val targetType: String,
    val reason: String,
    val notes: String?,
    val content: String?,
    val linkKey: String?,
    val isThread: Boolean,
)

internal fun buildPendingReport(
    host: String,
    targetId: String,
    targetType: String,
    reason: String,
    notes: String?,
    decryptedText: String,
    availableLinkKey: String?,
    passphraseProtected: Boolean,
    isThread: Boolean,
    includeText: Boolean = false,
    includeKey: Boolean = false,
): PendingReport = PendingReport(
    host = host,
    targetId = targetId,
    targetType = targetType,
    reason = reason.trim(),
    notes = notes?.takeIf(String::isNotBlank),
    content = decryptedText.take(RelayClient.MAX_REPORT_TEXT_CHARS).takeIf { includeText },
    linkKey = availableLinkKey.takeIf { includeKey && !passphraseProtected && !it.isNullOrBlank() },
    isThread = isThread,
)

@Composable
internal fun ReportConfirmationDialog(
    report: PendingReport,
    onConfirm: () -> Unit,
    onReview: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onReview,
        title = { Text(stringResource(R.string.report_confirm_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(if (report.isThread) {
                    R.string.report_confirm_thread_message
                } else {
                    R.string.report_confirm_message
                }))
                Text(stringResource(R.string.report_reason) + ": " + report.reason)
                report.notes?.let { Text(stringResource(R.string.report_note) + ": " + it) }
                if (report.content != null) {
                    Text(stringResource(R.string.thread_report_preview, report.content.length))
                }
                if (report.linkKey != null) Text(stringResource(R.string.report_include_key))
            }
        },
        confirmButton = {
            TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onConfirm) {
                Text(stringResource(R.string.report_confirm_send))
            }
        },
        dismissButton = {
            TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onReview) {
                Text(stringResource(R.string.report_cancel))
            }
        },
    )
}
