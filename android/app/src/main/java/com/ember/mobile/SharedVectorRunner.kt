package com.ember.mobile

import android.content.Context
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.FfiException
import com.ember.mobile.core.validateLink
import org.json.JSONObject

internal object SharedVectorRunner {
    fun run(context: Context): Int {
        val root = context.assets.open("drop-v1.json").bufferedReader().use { reader ->
            JSONObject(reader.readText())
        }
        var passed = 0

        val decryptCases = root.getJSONArray("decrypt_cases")
        for (index in 0 until decryptCases.length()) {
            val vector = decryptCases.getJSONObject(index)
            val key = hexToBytes(vector.getString("K_hex"))
            val ciphertext = hexToBytes(vector.getString("ciphertext_hex"))
            val crypto = try {
                EmberCrypto.fromKeyBytes(key)
            } finally {
                key.fill(0)
            }
            try {
                val actual = crypto.decryptDropPayload(ciphertext)
                try {
                    val expected = vector.getJSONObject("expect")
                    check(actual.kind == expected.getString("kind"))
                    check(actual.name == expected.getString("name"))
                    check(actual.mime == expected.getString("mime"))
                    check(actual.size == expected.getLong("size").toULong())
                    check(bytesToHex(actual.content) == expected.getString("content_hex"))
                } finally {
                    actual.content.fill(0)
                }
            } finally {
                crypto.close()
            }
            passed += 1
        }

        val negativeCases = root.getJSONArray("negative_cases")
        for (index in 0 until negativeCases.length()) {
            val vector = negativeCases.getJSONObject(index)
            if (vector.has("K_hex") && vector.has("ciphertext_hex")) {
                val key = hexToBytes(vector.getString("K_hex"))
                val crypto = try {
                    EmberCrypto.fromKeyBytes(key)
                } finally {
                    key.fill(0)
                }
                try {
                    val ciphertext = hexToBytes(vector.getString("ciphertext_hex"))
                    var rejected = false
                    try {
                        val unexpected = crypto.decryptDropPayload(ciphertext)
                        unexpected.content.fill(0)
                    } catch (_: FfiException) {
                        rejected = true
                    }
                    check(rejected) { "Negative drop vector ${vector.getString("name")} was accepted" }
                } finally {
                    crypto.close()
                }
            } else if (vector.has("payload_b64url")) {
                val link = "https://ember.example/d/AAAAAAAAAAAAAAAAAAAAAA#${vector.getString("payload_b64url")}"
                var rejected = false
                try {
                    validateLink(link)
                } catch (_: FfiException) {
                    rejected = true
                }
                check(rejected) { "Negative link vector ${vector.getString("name")} was accepted" }
            } else {
                error("Unsupported negative vector ${vector.getString("name")}")
            }
            passed += 1
        }

        return passed
    }

    private fun hexToBytes(hex: String): ByteArray {
        require(hex.length % 2 == 0) { "Invalid hex vector length" }
        return ByteArray(hex.length / 2) { index ->
            hex.substring(index * 2, index * 2 + 2).toInt(16).toByte()
        }
    }

    private fun bytesToHex(bytes: ByteArray): String = buildString(bytes.size * 2) {
        for (byte in bytes) {
            val value = byte.toInt() and 0xff
            append(HEX[value ushr 4])
            append(HEX[value and 0x0f])
        }
    }

    private const val HEX = "0123456789abcdef"
}
