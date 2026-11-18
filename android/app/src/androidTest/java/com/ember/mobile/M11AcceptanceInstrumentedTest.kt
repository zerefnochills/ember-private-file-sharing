package com.ember.mobile

import android.content.res.Configuration
import androidx.appcompat.app.AppCompatDelegate
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean

@RunWith(AndroidJUnit4::class)
class M11AcceptanceInstrumentedTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun firstLaunchRequiresLocalTermsAcceptanceBeforeTransferControls() {
        AppCompatDelegate.setApplicationLocales(androidx.core.os.LocaleListCompat.forLanguageTags("en"))
        compose.activity.getSharedPreferences("ember_app", android.content.Context.MODE_PRIVATE)
            .edit().remove("terms_accepted_v1").commit()
        compose.activityRule.scenario.recreate()
        compose.waitForIdle()

        compose.onNodeWithText(compose.activity.getString(R.string.terms_title)).assertIsDisplayed()
        assertTrue(
            compose.onAllNodes(hasText(compose.activity.getString(R.string.action_encrypt_send)))
                .fetchSemanticsNodes().isEmpty(),
        )
        val acceptance = compose.onNodeWithContentDescription(
            compose.activity.getString(R.string.legal_checkbox_accessibility),
        )
        acceptance.performScrollTo().assertIsDisplayed().performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.terms_accept_button))
            .performScrollTo().assertIsDisplayed().performClick()
        compose.waitUntil(10_000) {
            compose.activity.getSharedPreferences("ember_app", android.content.Context.MODE_PRIVATE)
                .getBoolean("terms_accepted_v1", false) &&
                compose.onAllNodes(hasText(compose.activity.getString(R.string.terms_title)))
                    .fetchSemanticsNodes().isEmpty()
        }
        assertTrue(
            compose.activity.getSharedPreferences("ember_app", android.content.Context.MODE_PRIVATE)
                .getBoolean("terms_accepted_v1", false),
        )
        compose.onNodeWithText(compose.activity.getString(R.string.action_encrypt_send))
            .performScrollTo().assertIsDisplayed()
    }

    @Test
    fun appLanguageSettingPersistsEnglishAndHindiLocales() {
        try {
            AppCompatDelegate.setApplicationLocales(androidx.core.os.LocaleListCompat.forLanguageTags("hi"))
            compose.waitForIdle()
            assertEquals("hi", AppCompatDelegate.getApplicationLocales().get(0)?.language)
            assertEquals("hi", compose.activity.resources.configuration.locales[0].language)

            AppCompatDelegate.setApplicationLocales(androidx.core.os.LocaleListCompat.forLanguageTags("en"))
            compose.waitForIdle()
            assertEquals("en", AppCompatDelegate.getApplicationLocales().get(0)?.language)
        } finally {
            AppCompatDelegate.setApplicationLocales(androidx.core.os.LocaleListCompat.forLanguageTags("en"))
        }
    }

    @Test
    fun termsLayoutKeepsFullHindiTextAtLargeFontScalesOn720By1280Viewport() {
        val activity = compose.activity
        val initial = Configuration(activity.resources.configuration)
        val hindiConfiguration = Configuration(initial).apply {
            setLocale(Locale.forLanguageTag("hi-IN"))
            screenWidthDp = 360
            screenHeightDp = 640
            densityDpi = 320
        }
        val hindiContext = activity.createConfigurationContext(hindiConfiguration)
        val termsText = loadBundledDocument(hindiContext, "terms-of-use-hi.md")

        for (fontScale in listOf(1.3f, 2.0f)) {
            val scaledConfiguration = Configuration(hindiConfiguration).apply { this.fontScale = fontScale }
            val scaledContext = activity.createConfigurationContext(scaledConfiguration)
            compose.activityRule.scenario.onActivity { activity ->
                activity.setContentView(ComposeView(activity).apply {
                    setContent {
                        CompositionLocalProvider(
                            LocalContext provides scaledContext,
                            LocalConfiguration provides scaledConfiguration,
                            LocalDensity provides Density(density = 2f, fontScale = fontScale),
                        ) {
                            MaterialTheme {
                                Box(Modifier.width(360.dp).height(640.dp)) {
                                    TermsAcceptanceScreen(onAccept = {}, onContact = {}, onPrivacy = {})
                                }
                            }
                        }
                    }
                })
            }
            compose.waitForIdle()

            val termsNode = compose.onNodeWithText(termsText, useUnmergedTree = true)
            termsNode.assertExists()
            val semanticsText = termsNode.fetchSemanticsNode().config[SemanticsProperties.Text]
                .joinToString("") { it.text }
            assertEquals("Terms text was truncated at fontScale=$fontScale", termsText, semanticsText)
            compose.onNodeWithText(scaledContext.getString(R.string.terms_accept_button)).fetchSemanticsNode()
            compose.onNodeWithText(scaledContext.getString(R.string.contact_grievance)).fetchSemanticsNode()
            compose.onNodeWithText(scaledContext.getString(R.string.privacy_policy)).fetchSemanticsNode()

            val allTextNodes = compose.onAllNodes(
                SemanticsMatcher.keyIsDefined(SemanticsProperties.Text),
                useUnmergedTree = true,
            ).fetchSemanticsNodes()
            val visibleBounds = allTextNodes.map { it.boundsInRoot }
                .filter { it.width > 0f && it.height > 0f && it.bottom > 0f && it.top < 1280f }
            for (i in visibleBounds.indices) {
                for (j in i + 1 until visibleBounds.size) {
                    val a = visibleBounds[i]
                    val b = visibleBounds[j]
                    val overlap = a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
                    assertFalse("Hindi text bounds overlap at fontScale=$fontScale: $a, $b", overlap)
                }
            }
        }
    }

    @Test
    fun threadReportWaitsForConfirmationAndOptionalFieldsAreOmittedByDefault() {
        val draft = buildPendingReport(
            host = "relay.example.invalid",
            targetId = "AAAAAAAAAAAAAAAAAAAAAA",
            targetType = "mailbox",
            reason = "abuse",
            notes = null,
            decryptedText = "amber-fox-4821: selected message",
            availableLinkKey = "selected-key",
            passphraseProtected = false,
            isThread = true,
        )
        assertNull(draft.content)
        assertNull(draft.linkKey)
        assertNull(draft.notes)

        val sent = AtomicBoolean(false)
        compose.activityRule.scenario.onActivity { activity ->
            activity.setContentView(ComposeView(activity).apply {
                setContent {
                    MaterialTheme {
                        ReportConfirmationDialog(
                            report = draft.copy(content = "amber-fox-4821: selected message"),
                            onConfirm = { sent.set(true) },
                            onReview = {},
                        )
                    }
                }
            })
        }
        compose.onNodeWithText(compose.activity.getString(R.string.report_confirm_thread_message)).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.report_confirm_send)).assertIsDisplayed()
        assertFalse("Report content was sent before confirmation", sent.get())
        compose.onNodeWithText(compose.activity.getString(R.string.report_confirm_send)).performClick()
        assertTrue(sent.get())
    }

    @Test
    fun threadScreenClosesWhenServerExpiryHasElapsed() {
        val closed = AtomicBoolean(false)
        val expiredAt = (System.currentTimeMillis() / 1_000 - 1).toULong()
        compose.activityRule.scenario.onActivity { activity ->
            activity.setContentView(ComposeView(activity).apply {
                setContent {
                    MaterialTheme {
                        ThreadScreen(
                            state = ThreadUiState(expiryEpochSeconds = expiredAt),
                            onSend = {},
                            onClose = { closed.set(true) },
                            onReport = {},
                            onBlock = {},
                        )
                    }
                }
            })
        }
        compose.waitUntil(5_000) { closed.get() }
        assertTrue("The thread screen did not close when expires_at elapsed", closed.get())
    }
}
