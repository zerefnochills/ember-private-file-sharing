package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ember.mobile.core.EmberCrypto
import com.ember.mobile.core.coreVersion
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RustBridgeInstrumentedTest {
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
}
