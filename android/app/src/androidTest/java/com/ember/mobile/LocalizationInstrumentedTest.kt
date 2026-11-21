package com.ember.mobile

import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.junit.Rule
import org.junit.Test

class LocalizationInstrumentedTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun longMessagesWrapAndRemainCompleteOnNarrowScreen() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val warning = context.getString(R.string.burn_dialog_message)
        val reportNotice = context.getString(R.string.report_fields_notice)

        compose.setContent {
            Column(
                modifier = Modifier.width(280.dp).verticalScroll(rememberScrollState()),
            ) {
                Text(stringResource(R.string.burn_dialog_message))
                Text(stringResource(R.string.report_fields_notice))
            }
        }

        compose.onNodeWithText(warning).assertIsDisplayed()
        compose.onNodeWithText(reportNotice).assertIsDisplayed()
    }
}
