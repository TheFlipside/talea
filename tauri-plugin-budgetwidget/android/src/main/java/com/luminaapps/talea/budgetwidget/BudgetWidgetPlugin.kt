package com.luminaapps.talea.budgetwidget

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import org.json.JSONArray
import org.json.JSONObject

@InvokeArg
class AccountHealthArg {
    var id: String = ""
    var name: String = ""
    var fraction: Double = 0.0
    var overspent: Boolean = false
}

@InvokeArg
class PublishArgs {
    var accounts: List<AccountHealthArg> = emptyList()
}

/**
 * Receives the abstract per-account health from the app, stores it for the
 * widget to read, and nudges any placed widgets to redraw.
 */
@TauriPlugin
class BudgetWidgetPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun publishHealth(invoke: Invoke) {
        val args = invoke.parseArgs(PublishArgs::class.java)
        val array = JSONArray()
        for (account in args.accounts) {
            array.put(
                JSONObject()
                    .put("id", account.id)
                    .put("name", account.name)
                    .put("fraction", account.fraction)
                    .put("overspent", account.overspent)
            )
        }
        val context = activity.applicationContext
        WidgetData.storeAccounts(context, array.toString())
        BudgetWidgetProvider.refreshAll(context)
        invoke.resolve()
    }
}
