package com.djilivebridge.android

import android.content.Context
import androidx.core.content.edit

/** Non-sensitive UI settings. Stream keys never go here; they stay in [DestinationProfileStore]. */
internal class UiPreferences(context: Context) {
    private val preferences = context.getSharedPreferences(FILE_NAME, Context.MODE_PRIVATE)

    var guideCompleted: Boolean
        get() = preferences.getBoolean(KEY_GUIDE_COMPLETED, false)
        set(value) = preferences.edit { putBoolean(KEY_GUIDE_COMPLETED, value) }

    var theme: ThemeChoice
        get() = ThemeChoice.fromStorage(preferences.getString(KEY_THEME, null))
        set(value) = preferences.edit { putString(KEY_THEME, value.storageValue) }

    var pictureFit: PictureFit
        get() = PictureFit.fromStorage(preferences.getString(KEY_PICTURE_FIT, null))
        set(value) = preferences.edit { putString(KEY_PICTURE_FIT, value.storageValue) }

    private companion object {
        const val FILE_NAME = "ui_preferences"

        // Bumped when the guide changes so everyone sees the new one once.
        const val KEY_GUIDE_COMPLETED = "guide_completed_v3"
        const val KEY_THEME = "theme"
        const val KEY_PICTURE_FIT = "picture_fit"
    }
}
