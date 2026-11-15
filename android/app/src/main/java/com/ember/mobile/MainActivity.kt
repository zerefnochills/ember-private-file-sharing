package com.ember.mobile

import android.content.Intent
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf

class MainActivity : ComponentActivity() {
    private val incomingLink = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setFlags(
            WindowManager.LayoutParams.FLAG_SECURE,
            WindowManager.LayoutParams.FLAG_SECURE,
        )
        incomingLink.value = readSharedText(intent)
        setContent {
            MaterialTheme {
                EmberHome(initialLink = incomingLink.value) { link -> shareEmberLink(this, link) }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        acceptIncomingIntent(intent)
    }

    internal fun acceptIncomingIntent(intent: Intent) {
        setIntent(intent)
        incomingLink.value = readSharedText(intent).orEmpty()
    }

    private fun readSharedText(intent: Intent?): String? = when (intent?.action) {
        Intent.ACTION_SEND -> intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        Intent.ACTION_VIEW -> intent.dataString
        else -> null
    }
}
