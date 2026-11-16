package com.ember.mobile

import androidx.test.core.app.ApplicationProvider
import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DropBlocklistInstrumentedTest {
    @Test
    fun blocksOnlyTheExactDropAndCanBeRemoved() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val key = DropBlocklist.key("relay.ember.example", "AAAAAAAAAAAAAAAAAAAAAA")
        DropBlocklist.update(context, DropBlocklist.read(context), key, blocked = false)

        val blocked = DropBlocklist.update(context, DropBlocklist.read(context), key, blocked = true)
        assertTrue(key in blocked)
        assertTrue(key in DropBlocklist.read(context))
        assertFalse("other.ember.example/AAAAAAAAAAAAAAAAAAAAAA" in blocked)

        val unblocked = DropBlocklist.update(context, DropBlocklist.read(context), key, blocked = false)
        assertFalse(key in unblocked)
    }
}
