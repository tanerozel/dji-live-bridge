package com.djilivebridge.android

import android.content.Context
import androidx.core.content.edit

/** Non-sensitive UI flags. Stream keys never go here; they stay in [DestinationProfileStore]. */
internal class UiPreferences(context: Context) {
    private val preferences = context.getSharedPreferences(FILE_NAME, Context.MODE_PRIVATE)

    var guideCompleted: Boolean
        get() = preferences.getBoolean(KEY_GUIDE_COMPLETED, false)
        set(value) = preferences.edit { putBoolean(KEY_GUIDE_COMPLETED, value) }

    private companion object {
        const val FILE_NAME = "ui_preferences"
        const val KEY_GUIDE_COMPLETED = "guide_completed_v1"
    }
}
