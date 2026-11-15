package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.EmberDecryptStream
import com.ember.mobile.core.EmberEncryptStream
import com.ember.mobile.core.coreVersion
import com.ember.mobile.core.inspectLink
import com.ember.mobile.core.solvePow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RustBridgeInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun generatedBindingsCallRustCore() {
        assertFalse(coreVersion().isBlank())
        val core = EmberCrypto.generate()
        core.use {
            val now = System.currentTimeMillis() / 1000
            val ciphertext = it.encryptText("shared bridge vector", now.toULong(), (now + 60).toULong())
            assertEquals("shared bridge vector", it.decryptText(ciphertext))
        }
    }

    @Test
    fun debugScreenRunsEverySharedVectorThroughRustBindings() {
        composeRule.onNodeWithText("Run shared test vectors").performClick()
        composeRule.waitUntil(120_000) {
            composeRule.onAllNodesWithText("All 13 vector cases passed").fetchSemanticsNodes().isNotEmpty()
        }
    }

    @Test
    fun generatedBindingsBuildAndOpenLinksAndSolvePow() {
        val crypto = EmberCrypto.generate()
        crypto.use { key ->
            val now = System.currentTimeMillis() / 1000
            val ciphertext = key.encryptText(
                "M10 link binding check",
                now.toULong(),
                (now + 60).toULong(),
            )
            val blobId = "AAAAAAAAAAAAAAAAAAAAAA"
            val plainLink = key.buildLink("ember.example.com", blobId)
            val plainInfo = inspectLink(plainLink)
            assertEquals("ember.example.com", plainInfo.host)
            assertEquals(blobId, plainInfo.blobId)
            assertFalse(plainInfo.passphraseProtected)

            val plainOpened = EmberCrypto.fromLink(plainLink, null)
            plainOpened.use { assertEquals("M10 link binding check", it.decryptText(ciphertext)) }

            val phrase = "battery staple phrase"
            val protectedLink = key.buildProtectedLink("ember.example.com", blobId, phrase)
            assertTrue(inspectLink(protectedLink).passphraseProtected)
            val protectedOpened = EmberCrypto.fromLink(protectedLink, phrase)
            protectedOpened.use { assertEquals("M10 link binding check", it.decryptText(ciphertext)) }

            val burnCiphertext = key.encryptTextWithOptions(
                "burn metadata check",
                true,
                now.toULong(),
                (now + 60).toULong(),
            )
            val burnDrop = key.decryptDropPayload(burnCiphertext)
            assertTrue(burnDrop.burn)
            burnDrop.content.fill(0)
        }

        // The core validates the nonce in its host test; this also exercises the generated Kotlin ABI.
        solvePow("android-m10-pow", 8u)
    }

    @Test
    fun generatedStreamingBindingsRoundtripFileAndAuthenticatedName() {
        val crypto = EmberCrypto.generate()
        val now = System.currentTimeMillis() / 1_000
        val content = "file from device".toByteArray()
        val encrypt = EmberEncryptStream(
            crypto,
            "../private.bin",
            "application/octet-stream",
            content.size.toULong(),
            true,
            now.toULong(),
            (now + 60).toULong(),
        )
        try {
            val part0 = encrypt.takePart0()
            val parts = encrypt.update(content.copyOf()) + encrypt.finish()
            val decrypt = EmberDecryptStream(crypto, part0)
            try {
                val metadata = decrypt.metadata()
                assertEquals("../private.bin", metadata.name)
                assertEquals("private.bin", metadata.safeName)
                assertEquals(content.size.toULong(), metadata.size)
                assertTrue(metadata.burn)
                val chunk = decrypt.pushPart(parts.single())
                assertTrue(chunk.isFinal)
                assertEquals(content.toList(), chunk.plaintext.toList())
                chunk.plaintext.fill(0)
                decrypt.finish()
            } finally {
                decrypt.close()
                part0.fill(0)
                parts.forEach { it.fill(0) }
            }
        } finally {
            encrypt.close()
            crypto.close()
            content.fill(0)
        }
    }

    @Test
    fun sendReceiveNavigationShowsLinkAndQrControls() {
        composeRule.onNodeWithText("Receive").performClick()
        assertTrue(composeRule.onAllNodesWithText("Scan QR code").fetchSemanticsNodes().isNotEmpty())
        assertTrue(composeRule.onAllNodesWithText("Check link").fetchSemanticsNodes().isNotEmpty())
        assertTrue(composeRule.onAllNodesWithText("Paste or share an Ember link").fetchSemanticsNodes().isNotEmpty())

        composeRule.onNodeWithText("Send").performClick()
        assertTrue(composeRule.onAllNodesWithText("Encrypt and send").fetchSemanticsNodes().isNotEmpty())
        assertTrue(composeRule.onAllNodesWithText("Relay host").fetchSemanticsNodes().isNotEmpty())
        assertTrue(composeRule.onAllNodesWithText("Choose a file to encrypt and send").fetchSemanticsNodes().isNotEmpty())
        composeRule.onNodeWithText("Receive").performClick()
        assertTrue(composeRule.onAllNodesWithText("Check link for file").fetchSemanticsNodes().isNotEmpty())
    }
}
