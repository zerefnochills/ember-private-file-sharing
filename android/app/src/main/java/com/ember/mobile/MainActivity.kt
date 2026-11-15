package com.ember.mobile

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.coreVersion

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setFlags(
            WindowManager.LayoutParams.FLAG_SECURE,
            WindowManager.LayoutParams.FLAG_SECURE,
        )
        setContent {
            MaterialTheme {
                var result by remember { mutableStateOf("Rust core ${coreVersion()} loaded") }
                Column(
                    modifier = Modifier.fillMaxSize().padding(24.dp),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("Ember M9 bridge check")
                    Text(result, modifier = Modifier.padding(vertical = 16.dp))
                    Button(onClick = {
                        result = "Running shared test vectors…"
                        Thread {
                            val outcome = runCatching { SharedVectorRunner.run(applicationContext) }
                            runOnUiThread {
                                result = outcome.fold(
                                    onSuccess = { "All $it vector cases passed" },
                                    onFailure = { "Vector check failed: ${it.javaClass.simpleName}" },
                                )
                            }
                        }.start()
                    }) {
                        Text("Run shared test vectors")
                    }
                    Button(onClick = {
                        result = runCatching {
                            val core = EmberCrypto.generate()
                            core.use {
                                val now = System.currentTimeMillis() / 1000
                                val ciphertext = it.encryptText(
                                    "M9 native bridge works",
                                    now.toULong(),
                                    (now + 60).toULong(),
                                )
                                check(it.decryptText(ciphertext) == "M9 native bridge works")
                                ""
                            }
                        }.fold(
                            onSuccess = { "Rust round trip passed" },
                            onFailure = { "Rust bridge failed: ${it.javaClass.simpleName}" },
                        )
                    }) {
                        Text("Run local encryption test")
                    }
                }
            }
        }
    }
}
