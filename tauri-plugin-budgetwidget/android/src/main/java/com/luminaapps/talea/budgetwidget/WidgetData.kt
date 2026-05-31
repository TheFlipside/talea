package com.luminaapps.talea.budgetwidget

import android.content.Context
import org.json.JSONArray
import org.json.JSONException

/**
 * Reads/writes the abstract budget-health snapshot the widget renders.
 *
 * Two kinds of data live in one [SharedPreferences] file:
 *  - [KEY_ACCOUNTS]: the published per-account health (written by the app via
 *    the plugin), a JSON array of [Health].
 *  - one entry per placed widget (`widget_<id>` → account id), written by the
 *    config activity, recording which account that widget tracks.
 *
 * No monetary figure is ever stored here — only an abstract ring fraction.
 */
internal object WidgetData {
    const val PREFS = "talea_widget"
    private const val KEY_ACCOUNTS = "accounts"

    /** One account's abstract health for the current month. */
    data class Health(
        val id: String,
        val name: String,
        val fraction: Double,
        val overspent: Boolean,
    )

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun widgetKey(widgetId: Int) = "widget_$widgetId"

    /** Stores the published snapshot (a JSON array string from the plugin). */
    fun storeAccounts(context: Context, accountsJson: String) {
        prefs(context).edit().putString(KEY_ACCOUNTS, accountsJson).apply()
    }

    /** The published per-account health, or an empty list if none/malformed. */
    fun accounts(context: Context): List<Health> {
        val json = prefs(context).getString(KEY_ACCOUNTS, null) ?: return emptyList()
        return try {
            val arr = JSONArray(json)
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                Health(
                    id = o.getString("id"),
                    name = o.optString("name"),
                    fraction = o.optDouble("fraction", 0.0),
                    overspent = o.optBoolean("overspent", false),
                )
            }
        } catch (_: JSONException) {
            emptyList()
        }
    }

    /** The health for the account a given widget tracks, if still present. */
    fun healthFor(context: Context, widgetId: Int): Health? {
        val accountId = prefs(context).getString(widgetKey(widgetId), null) ?: return null
        return accounts(context).firstOrNull { it.id == accountId }
    }

    fun setWidgetAccount(context: Context, widgetId: Int, accountId: String) {
        prefs(context).edit().putString(widgetKey(widgetId), accountId).apply()
    }

    fun clearWidget(context: Context, widgetId: Int) {
        prefs(context).edit().remove(widgetKey(widgetId)).apply()
    }
}
