package com.ember.mobile

import com.ember.mobile.core.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import org.json.JSONObject

class EmberCoreTest {

    private fun hexToBytes(hex: String): ByteArray {
        val result = ByteArray(hex.length / 2)
        for (i in 0 until hex.length step 2) {
            result[i / 2] = hex.substring(i, i + 2).toInt(16).toByte()
        }
        return result
    }

    private fun bytesToHex(bytes: ByteArray): String {
        val hexChars = "0123456789abcdef"
        val result = StringBuilder(bytes.size * 2)
        for (b in bytes) {
            val v = b.toInt() and 0xFF
            result.append(hexChars[v ushr 4])
            result.append(hexChars[v and 0x0F])
        }
        return result.toString()
    }

    @Test
    fun testCoreVersion() {
        val version = coreVersion()
        assertNotNull("Version should not be null", version)
        assertTrue("Version should not be empty", version.isNotEmpty())
    }

    @Test
    fun testTextRoundtrip() {
        val crypto = EmberCrypto.generate()
        val now = 1000L.toULong()
        val expiry = 2000L.toULong()
        val ciphertext = crypto.encryptText("Hello from Android Kotlin test", now, expiry)
        assertTrue("Ciphertext should not be empty", ciphertext.isNotEmpty())
        val decrypted = crypto.decryptText(ciphertext)
        assertEquals("Hello from Android Kotlin test", decrypted)
    }

    @Test
    fun testFileRoundtrip() {
        val crypto = EmberCrypto.generate()
        val payload = byteArrayOf(1, 2, 3, 4, 5)
        val now = 1000L.toULong()
        val expiry = 2000L.toULong()
        val ciphertext = crypto.encryptFile("hello.bin", "application/octet-stream", payload, true, now, expiry)
        val decrypted = crypto.decryptDropPayload(ciphertext)
        assertEquals("file", decrypted.kind)
        assertEquals("hello.bin", decrypted.name)
        assertEquals("application/octet-stream", decrypted.mime)
        assertEquals(5L.toULong(), decrypted.size)
        assertTrue(decrypted.burn)
        assertArrayEquals(payload, decrypted.content)
    }

    @Test
    fun testInvalidKeyLengthRejected() {
        assertThrows(FfiException::class.java) {
            EmberCrypto.fromKeyBytes(ByteArray(31))
        }
        assertThrows(FfiException::class.java) {
            EmberCrypto.fromKeyBytes(ByteArray(33))
        }
    }

    @Test
    fun testInvalidExpiryRejected() {
        val crypto = EmberCrypto.generate()
        assertThrows(FfiException::class.java) {
            crypto.encryptText("test", 2000L.toULong(), 1000L.toULong())
        }
    }

    @Test
    fun testSharedVectorsDecryptCases() {
        val possiblePaths = listOf(
            File("../../testvectors/drop-v1.json"),
            File("../testvectors/drop-v1.json"),
            File("testvectors/drop-v1.json")
        )
        val vectorFile = possiblePaths.firstOrNull { it.exists() }
            ?: throw IllegalStateException("Could not find testvectors/drop-v1.json")

        val root = JSONObject(vectorFile.readText())
        val cases = root.getJSONArray("decrypt_cases")

        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val name = c.getString("name")
            val keyBytes = hexToBytes(c.getString("K_hex"))
            val ciphertext = hexToBytes(c.getString("ciphertext_hex"))
            val expect = c.getJSONObject("expect")

            val crypto = EmberCrypto.fromKeyBytes(keyBytes)
            val decrypted = crypto.decryptDropPayload(ciphertext)

            assertEquals("Case $name: kind mismatch", expect.getString("kind"), decrypted.kind)
            assertEquals("Case $name: name mismatch", expect.getString("name"), decrypted.name)
            assertEquals("Case $name: size mismatch", expect.getLong("size").toULong(), decrypted.size)

            val expectedContentHex = expect.getString("content_hex")
            val actualContentHex = bytesToHex(decrypted.content)
            assertEquals("Case $name: content mismatch", expectedContentHex, actualContentHex)
        }
    }

    @Test
    fun testSharedVectorsNegativeCases() {
        val possiblePaths = listOf(
            File("../../testvectors/drop-v1.json"),
            File("../testvectors/drop-v1.json"),
            File("testvectors/drop-v1.json")
        )
        val vectorFile = possiblePaths.firstOrNull { it.exists() }
            ?: throw IllegalStateException("Could not find testvectors/drop-v1.json")

        val root = JSONObject(vectorFile.readText())
        val cases = root.getJSONArray("negative_cases")

        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val name = c.getString("name")
            if (c.has("K_hex") && c.has("ciphertext_hex")) {
                val keyBytes = hexToBytes(c.getString("K_hex"))
                val ciphertext = hexToBytes(c.getString("ciphertext_hex"))
                val crypto = EmberCrypto.fromKeyBytes(keyBytes)
                try {
                    crypto.decryptDropPayload(ciphertext)
                    fail("Negative case $name should have failed decryption")
                } catch (e: FfiException) {
                    // Expected to fail
                }
            } else if (c.has("payload_b64url")) {
                val invalidLink = "https://ember.example/d/AAAAAAAAAAAAAAAAAAAAAA#${c.getString("payload_b64url")}"
                assertThrows(FfiException::class.java) {
                    validateLink(invalidLink)
                }
            }
        }
    }
}
