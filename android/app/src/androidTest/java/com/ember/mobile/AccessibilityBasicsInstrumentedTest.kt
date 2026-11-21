package com.ember.mobile

import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Before
import org.junit.Test
import kotlin.math.pow

class AccessibilityBasicsInstrumentedTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Before
    fun acceptTermsForExistingMainScreenChecks() {
        compose.activity.getSharedPreferences("ember_app", android.content.Context.MODE_PRIVATE)
            .edit().putBoolean("terms_accepted_v1", true).commit()
        compose.activityRule.scenario.recreate()
        compose.waitForIdle()
    }

    @Test
    fun primaryControlsExposeLabelsAndMinimumTouchTargets() {
        val context = compose.activity

        assertMinimumTouchTarget(compose.onNodeWithText("Send"))
        assertMinimumTouchTarget(
            compose.onNodeWithContentDescription(context.getString(R.string.burn_after_view)),
        )
    }

    @Test
    fun defaultThemeTextAndButtonColorsMeetTextContrast() {
        val colors = lightColorScheme()

        assertTrue("Primary button contrast is below 4.5:1", contrast(colors.primary, colors.onPrimary) >= 4.5)
        assertTrue("Body text contrast is below 4.5:1", contrast(colors.surface, colors.onSurface) >= 4.5)
    }

    private fun assertMinimumTouchTarget(node: SemanticsNodeInteraction) {
        node.performScrollTo()
        node.assertIsDisplayed()
        val bounds = node.fetchSemanticsNode().boundsInRoot
        val widthDp = with(compose.density) { bounds.width.toDp().value }
        val heightDp = with(compose.density) { bounds.height.toDp().value }
        assertTrue("Touch target is narrower than 48 dp", widthDp >= 48f)
        assertTrue("Touch target is shorter than 48 dp", heightDp >= 48f)
    }

    private fun contrast(background: Color, foreground: Color): Double {
        val high = maxOf(luminance(background), luminance(foreground))
        val low = minOf(luminance(background), luminance(foreground))
        return (high + 0.05) / (low + 0.05)
    }

    private fun luminance(color: Color): Double {
        fun channel(value: Float): Double {
            val component = value.toDouble()
            return if (component <= 0.04045) component / 12.92
            else ((component + 0.055) / 1.055).pow(2.4)
        }
        return 0.2126 * channel(color.red) + 0.7152 * channel(color.green) + 0.0722 * channel(color.blue)
    }
}
