package com.ember.mobile

import android.content.Context

/** Local block list for exact drop IDs; it never stores a sender identity or link fragment. */
internal object DropBlocklist {
    private const val PREFERENCES = "ember_blocked_drops"
    private const val IDS = "ids"

    fun key(host: String, blobId: String): String = "${host.lowercase()}/$blobId"

    fun read(context: Context): Set<String> =
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            .getStringSet(IDS, mutableSetOf())
            .orEmpty()
            .toSet()

    fun update(context: Context, current: Set<String>, key: String, blocked: Boolean): Set<String> {
        val updated = current.toMutableSet().apply {
            if (blocked) add(key) else remove(key)
        }.toSet()
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            .edit()
            .putStringSet(IDS, updated)
            .apply()
        return updated
    }
}
