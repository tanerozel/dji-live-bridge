package com.streammydrone.app

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.net.toUri

/**
 * Opens a real browser, even when Android sends Instagram links to the Instagram app.
 * The web session, password, 2FA, cookies and page content stay in that browser. No WebView,
 * Javascript bridge, interception, callback URL, account permission or credential extraction.
 */
internal class InstagramBrowser {
    private var openedPackage: String? = null

    fun open(context: Context): UiText? {
        val manager = context.packageManager
        // This URL is only resolved locally, never opened or requested over the network.
        val genericIntent = Intent(Intent.ACTION_VIEW, "https://browser-check.invalid/".toUri())
            .addCategory(Intent.CATEGORY_BROWSABLE)
        val discovered = try {
            listOfNotNull(manager.resolveActivity(genericIntent, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo?.packageName) +
                manager.queryIntentActivities(genericIntent, PackageManager.MATCH_DEFAULT_ONLY).map { it.activityInfo.packageName }
        } catch (_: SecurityException) { emptyList() }
        // Package visibility/default-app filtering differs between Android vendors. Try launching
        // known browsers explicitly as well; startActivity itself decides availability. Never fall
        // back to an unqualified Instagram URL intent (it would open the Instagram app).
        val packages = instagramBrowserPackages(openedPackage, discovered)
        for (target in packages) {
            try {
                // Resume without navigating/reloading an already open Live Producer page.
                val resume = if (openedPackage == target) manager.getLaunchIntentForPackage(target) else null
                val intent = (resume ?: Intent(Intent.ACTION_VIEW, INSTAGRAM_SITE.toUri())
                    .addCategory(Intent.CATEGORY_BROWSABLE).setPackage(target))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT)
                context.startActivity(intent)
                openedPackage = target
                return null
            } catch (_: ActivityNotFoundException) { continue }
            catch (_: SecurityException) { continue }
        }
        return uiText(R.string.instagram_error_browser)
    }

    companion object {
        private const val INSTAGRAM_SITE = "https://www.instagram.com/"
    }
}
