package com.ember.mobile

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DeepLinkIntentInstrumentedTest {
    @Test
    fun handledLinkIsRemovedFromIntentAndSavedInstanceState() {
        val fragmentCanary = "fragment-canary-must-not-be-saved"
        val link = Uri.Builder()
            .scheme("https")
            .authority(BuildConfig.EMBER_LINK_HOST)
            .appendPath("d")
            .appendPath("AAAAAAAAAAAAAAAAAAAAAA")
            .fragment(fragmentCanary)
            .build()
        val incoming = Intent(Intent.ACTION_VIEW, link)

        val instrumentation = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(
            instrumentation.targetContext,
            MainActivity::class.java,
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        try {
            instrumentation.runOnMainSync {
                activity.acceptIncomingIntent(incoming)
            }

            instrumentation.runOnMainSync {
                assertNull("handled source intent must not retain its URL", incoming.data)
                assertNull("activity intent must not retain the incoming URL", activity.intent.data)
                val saved = Bundle()
                instrumentation.callActivityOnSaveInstanceState(activity, saved)
                assertFalse("saved state must not contain the URL fragment", bundleContains(saved, fragmentCanary))
                assertFalse("saved state must not contain the full URL", bundleContains(saved, link.toString()))
            }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
            instrumentation.waitForIdleSync()
        }
    }

    @Test
    fun browsableFilterMatchesOnlyConfiguredHttpsHostAndDropPath() {
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        fun resolves(url: String): Boolean = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            .setPackage(context.packageName)
            .resolveActivity(context.packageManager) != null

        assertTrue(resolves("https://${BuildConfig.EMBER_LINK_HOST}/d/AAAAAAAAAAAAAAAAAAAAAA#payload"))
        assertFalse(resolves("https://other.example/d/AAAAAAAAAAAAAAAAAAAAAA#payload"))
        assertFalse(resolves("http://${BuildConfig.EMBER_LINK_HOST}/d/AAAAAAAAAAAAAAAAAAAAAA#payload"))
        assertFalse(resolves("https://${BuildConfig.EMBER_LINK_HOST}/other/AAAAAAAAAAAAAAAAAAAAAA#payload"))
    }

    @Suppress("DEPRECATION")
    private fun bundleContains(bundle: Bundle, value: String): Boolean {
        for (key in bundle.keySet()) {
            val item = bundle.get(key) ?: continue
            if (item.toString().contains(value)) return true
            if (item is Bundle && bundleContains(item, value)) return true
        }
        return false
    }
}
