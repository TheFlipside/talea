package com.luminaapps.talea.budgetwidget

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity

/**
 * Runs when a widget is placed: lets the user pick which account it tracks.
 * The picker lists account names from the published snapshot ([WidgetData]); on
 * selection it records the choice, renders the widget, and returns RESULT_OK so
 * the system completes the placement (RESULT_CANCELED on back-out).
 */
class BudgetWidgetConfigActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // If the user backs out, the widget must not be added.
        setResult(Activity.RESULT_CANCELED)

        val widgetId = intent?.extras?.getInt(
            AppWidgetManager.EXTRA_APPWIDGET_ID,
            AppWidgetManager.INVALID_APPWIDGET_ID,
        ) ?: AppWidgetManager.INVALID_APPWIDGET_ID
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
            finish()
            return
        }

        val pad = (16 * resources.displayMetrics.density).toInt()
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
        }
        root.addView(
            TextView(this).apply {
                text = getString(R.string.budgetwidget_choose_account)
                textSize = 18f
                setPadding(0, 0, 0, pad)
            }
        )

        val accounts = WidgetData.accounts(this)
        if (accounts.isEmpty()) {
            // Nothing published yet (the app has never been opened). Explain and
            // dismiss rather than leaving a dead-end screen; the widget isn't
            // placed (RESULT_CANCELED) so the user can retry after opening Talea.
            Toast.makeText(this, R.string.budgetwidget_no_accounts, Toast.LENGTH_LONG).show()
            finish()
            return
        }

        val list = ListView(this).apply {
            adapter = ArrayAdapter(
                this@BudgetWidgetConfigActivity,
                android.R.layout.simple_list_item_1,
                accounts.map { it.name },
            )
            setOnItemClickListener { _, _, position, _ ->
                WidgetData.setWidgetAccount(
                    this@BudgetWidgetConfigActivity, widgetId, accounts[position].id,
                )
                BudgetWidgetProvider.render(
                    this@BudgetWidgetConfigActivity,
                    AppWidgetManager.getInstance(this@BudgetWidgetConfigActivity),
                    widgetId,
                )
                setResult(
                    Activity.RESULT_OK,
                    Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId),
                )
                finish()
            }
        }
        root.addView(
            list,
            LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ),
        )
        setContentView(root)
    }
}
