package com.ember.mobile

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
fun TermsAcceptanceScreen(
    onAccept: () -> Unit,
    onContact: () -> Unit,
    onPrivacy: () -> Unit,
) {
    var checked by remember { mutableStateOf(false) }
    val terms = rememberTermsDocument()
    val acceptDescription = stringResource(R.string.legal_checkbox_accessibility)

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.terms_title), style = MaterialTheme.typography.headlineSmall)
        Text(terms, style = MaterialTheme.typography.bodyMedium)
        TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onContact) {
            Text(stringResource(R.string.contact_grievance))
        }
        TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onPrivacy) {
            Text(stringResource(R.string.privacy_policy))
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Checkbox(
                checked = checked,
                onCheckedChange = { checked = it },
                modifier = Modifier.semantics {
                    contentDescription = acceptDescription
                },
            )
            Text(stringResource(R.string.terms_accept_checkbox))
        }
        Button(
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
            enabled = checked,
            onClick = onAccept,
        ) { Text(stringResource(R.string.terms_accept_button)) }
    }
}

@Composable
fun PrivacyPolicyDialog(onDismiss: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val policy = remember(context) {
        loadBundledDocument(context, "privacy-policy.md")
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.privacy_policy_title)) },
        text = {
            Column(modifier = Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState())) {
                Text(policy, style = MaterialTheme.typography.bodyMedium)
            }
        },
        confirmButton = {
            TextButton(modifier = Modifier.heightIn(min = 48.dp), onClick = onDismiss) {
                Text(stringResource(R.string.legal_close))
            }
        },
    )
}

fun loadBundledDocument(context: Context, name: String): String =
    context.assets.open(name).bufferedReader(Charsets.UTF_8).use { markdownToReadableText(it.readText()) }

@Composable
private fun rememberTermsDocument(): String {
    val context = androidx.compose.ui.platform.LocalContext.current
    return remember(context) { loadBundledDocument(context, "terms-of-use.md") }
}

private fun markdownToReadableText(markdown: String): String = markdown.lineSequence()
    .map { line ->
        val trimmed = line.trim()
        when {
            trimmed == "---" -> ""
            trimmed.startsWith("# ") -> trimmed.removePrefix("# ")
            trimmed.startsWith("## ") -> "\n${trimmed.removePrefix("## ").uppercase()}\n"
            trimmed.startsWith("- ") -> "• ${trimmed.removePrefix("- ")}"
            trimmed.startsWith("* ") -> "• ${trimmed.removePrefix("* ")}"
            trimmed.matches(Regex("\\d+\\. .*")) -> "• ${trimmed.substringAfter(". ")}"
            trimmed.startsWith("> ") -> trimmed.removePrefix("> ")
            else -> trimmed
        }
    }
    .joinToString("\n")
    .replace(Regex("\\*\\*(.*?)\\*\\*"), "$1")
    .replace(Regex("\\*(.*?)\\*"), "$1")
    .replace(Regex("`([^`]+)`"), "$1")
    .replace(Regex("\n{3,}"), "\n\n")
    .trim()
