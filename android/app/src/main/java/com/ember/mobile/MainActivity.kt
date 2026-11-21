package com.ember.mobile

import android.content.Intent
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.compose.setContent
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf

class MainActivity : AppCompatActivity() {
    private val incomingLink = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setFlags(
            WindowManager.LayoutParams.FLAG_SECURE,
            WindowManager.LayoutParams.FLAG_SECURE,
        )
        incomingLink.value = takeIncomingText(intent)
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
        incomingLink.value = takeIncomingText(intent).orEmpty()
    }

    private fun takeIncomingText(source: Intent?): String? {
        if (source?.action != Intent.ACTION_VIEW && source?.action != Intent.ACTION_SEND) return null
        val received = readSharedText(source)
        source.data = null
        source.removeExtra(Intent.EXTRA_TEXT)
        source.clipData = null
        setIntent(Intent())
        return received
    }

    private fun readSharedText(intent: Intent?): String? = when (intent?.action) {
        Intent.ACTION_SEND -> intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        Intent.ACTION_VIEW -> intent.dataString
        else -> null
    }
}
