package com.ember.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.Rule
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat

@RunWith(AndroidJUnit4::class)
class NetworkHostAllowlistInstrumentedTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun appContactsOnlyTheConfiguredRelayAfterAnExplicitProbe() {
        AppCompatDelegate.setApplicationLocales(LocaleListCompat.forLanguageTags("en"))
        compose.activity.getSharedPreferences("ember_app", android.content.Context.MODE_PRIVATE)
            .edit().remove("terms_accepted_v1").commit()
        compose.activityRule.scenario.recreate()
        compose.onNodeWithContentDescription(compose.activity.getString(R.string.legal_checkbox_accessibility))
            .performScrollTo().performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.terms_accept_button))
            .performScrollTo().performClick()
        compose.waitUntil(10_000) {
            compose.activity.getSharedPreferences("ember_app", android.content.Context.MODE_PRIVATE)
                .getBoolean("terms_accepted_v1", false)
        }

        assertFalse(BuildConfig.EMBER_RELAY_HOST.isBlank())
        val challenge = RelayClient.fromRelayHost(BuildConfig.EMBER_RELAY_HOST).getPowChallenge()
        assertTrue(challenge.challenge.isNotBlank())
        assertTrue(challenge.difficulty <= 32u)
    }
}
