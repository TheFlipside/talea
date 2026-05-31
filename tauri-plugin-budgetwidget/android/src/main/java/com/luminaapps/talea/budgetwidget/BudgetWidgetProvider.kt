package com.luminaapps.talea.budgetwidget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.widget.RemoteViews
import kotlin.math.roundToInt

/**
 * Renders the abstract budget-health ring for each placed widget. Reads the
 * account a widget tracks plus the published fraction from [WidgetData] and
 * draws the ring + percentage into a bitmap (RemoteViews can't draw arcs).
 */
class BudgetWidgetProvider : AppWidgetProvider() {

    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        for (id in ids) render(context, manager, id)
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        for (id in ids) WidgetData.clearWidget(context, id)
    }

    companion object {
        // Base ring size in dp; scaled by display density at render time so the
        // bitmap isn't upscaled (blurry) on high-DPI screens.
        private const val RING_DP = 132
        private const val RING_MIN_PX = 120
        private const val RING_MAX_PX = 512
        private const val STROKE_FRACTION = 0.11f // matches the in-app ring (11/100)

        private const val COLOR_TRACK = 0xFF28505C.toInt()
        private const val COLOR_ACCENT = 0xFF40C9A2.toInt()
        private const val COLOR_ERROR = 0xFFE5484D.toInt()
        private const val COLOR_TEXT = 0xFFF4F7F6.toInt()
        private const val COLOR_MUTED = 0xFF7A8C92.toInt()

        /** Redraw every placed widget (called after the app publishes fresh data). */
        fun refreshAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(
                ComponentName(context, BudgetWidgetProvider::class.java)
            )
            for (id in ids) render(context, manager, id)
        }

        fun render(context: Context, manager: AppWidgetManager, widgetId: Int) {
            val health = WidgetData.healthFor(context, widgetId)
            val fraction = (health?.fraction ?: 0.0).coerceIn(0.0, 1.0).toFloat()
            val overspent = health?.overspent ?: false
            val hasData = health != null

            val size = (RING_DP * context.resources.displayMetrics.density)
                .roundToInt()
                .coerceIn(RING_MIN_PX, RING_MAX_PX)

            val views = RemoteViews(context.packageName, R.layout.budget_widget)
            views.setImageViewBitmap(R.id.widget_ring, drawRing(size, fraction, overspent, hasData))
            views.setTextViewText(R.id.widget_name, health?.name ?: "")
            views.setContentDescription(R.id.widget_ring, "${(fraction * 100).roundToInt()}%")

            val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
            if (launch != null) {
                val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
                val pending = PendingIntent.getActivity(context, widgetId, launch, flags)
                views.setOnClickPendingIntent(R.id.widget_root, pending)
            }

            manager.updateAppWidget(widgetId, views)
        }

        private fun drawRing(size: Int, fraction: Float, overspent: Boolean, hasData: Boolean): Bitmap {
            val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bitmap)
            val stroke = size * STROKE_FRACTION
            val pad = stroke / 2f + 2f
            val rect = RectF(pad, pad, size - pad, size - pad)

            val track = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                strokeWidth = stroke
                color = COLOR_TRACK
            }
            canvas.drawArc(rect, 0f, 360f, false, track)

            if (hasData) {
                val value = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                    style = Paint.Style.STROKE
                    strokeWidth = stroke
                    strokeCap = Paint.Cap.ROUND
                    color = if (overspent) COLOR_ERROR else COLOR_ACCENT
                }
                canvas.drawArc(rect, -90f, 360f * fraction, false, value)
            }

            val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = if (hasData) COLOR_TEXT else COLOR_MUTED
                textAlign = Paint.Align.CENTER
                textSize = size * 0.26f
                isFakeBoldText = true
            }
            val label = if (hasData) "${(fraction * 100).roundToInt()}%" else "–"
            val baseline = size / 2f - (text.descent() + text.ascent()) / 2f
            canvas.drawText(label, size / 2f, baseline, text)

            return bitmap
        }
    }
}
