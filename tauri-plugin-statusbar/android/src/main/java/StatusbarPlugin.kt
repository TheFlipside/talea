package com.luminaapps.talea.statusbar

import android.app.Activity
import androidx.core.view.WindowCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class SetDarkArgs {
    var dark: Boolean = false
}

@TauriPlugin
class StatusbarPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun setDark(invoke: Invoke) {
        val args = invoke.parseArgs(SetDarkArgs::class.java)
        // Resolve from inside the UI-thread block, after the change is applied
        // (runOnUiThread posts asynchronously).
        activity.runOnUiThread {
            val window = activity.window
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            // Dark app theme → light (white) bar icons; light theme → dark icons.
            controller.isAppearanceLightStatusBars = !args.dark
            controller.isAppearanceLightNavigationBars = !args.dark
            invoke.resolve()
        }
    }
}
