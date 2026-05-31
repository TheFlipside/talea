import SwiftUI
import WidgetKit

struct HealthEntry: TimelineEntry {
  let date: Date
  let health: AccountHealth?
}

/// Reads the configured account's health from the App Group. There is no time
/// schedule — the app reloads timelines when it publishes fresh data.
struct Provider: AppIntentTimelineProvider {
  func placeholder(in context: Context) -> HealthEntry {
    HealthEntry(date: Date(), health: nil)
  }

  func snapshot(for configuration: SelectAccountIntent, in context: Context) async -> HealthEntry {
    HealthEntry(date: Date(), health: SharedHealth.health(for: configuration.account?.id))
  }

  func timeline(for configuration: SelectAccountIntent, in context: Context) async -> Timeline<
    HealthEntry
  > {
    let entry = HealthEntry(date: Date(), health: SharedHealth.health(for: configuration.account?.id))
    return Timeline(entries: [entry], policy: .never)
  }
}

/// The abstract ring (mirrors the in-app BudgetRing colors). No figures shown.
struct RingView: View {
  let health: AccountHealth?

  private var fraction: Double { min(max(health?.fraction ?? 0, 0), 1) }
  private var valueColor: Color {
    (health?.overspent ?? false)
      ? Color(red: 0.898, green: 0.282, blue: 0.302)  // error
      : Color(red: 0.251, green: 0.788, blue: 0.635)  // accent
  }

  var body: some View {
    VStack(spacing: 6) {
      ZStack {
        Circle().stroke(Color(red: 0.157, green: 0.314, blue: 0.361), lineWidth: 11)
        if health != nil {
          Circle()
            .trim(from: 0, to: fraction)
            .stroke(valueColor, style: StrokeStyle(lineWidth: 11, lineCap: .round))
            .rotationEffect(.degrees(-90))
        }
        Text(health != nil ? "\(Int((fraction * 100).rounded()))%" : "–")
          .font(.system(size: 22, weight: .bold))
          .foregroundColor(health != nil ? Color(red: 0.957, green: 0.969, blue: 0.965) : .gray)
      }
      if let name = health?.name, !name.isEmpty {
        Text(name)
          .font(.caption)
          .foregroundColor(Color(red: 0.957, green: 0.969, blue: 0.965))
          .lineLimit(1)
      }
    }
    .padding()
  }
}

struct TaleaWidgetEntryView: View {
  var entry: Provider.Entry

  var body: some View {
    RingView(health: entry.health)
      .containerBackground(Color(red: 0.071, green: 0.180, blue: 0.220), for: .widget)
  }
}

struct TaleaWidget: Widget {
  let kind = "TaleaWidget"

  var body: some WidgetConfiguration {
    AppIntentConfiguration(kind: kind, intent: SelectAccountIntent.self, provider: Provider()) {
      entry in
      TaleaWidgetEntryView(entry: entry)
    }
    .configurationDisplayName("Budget ring")
    .description("An abstract ring of an account's budget health.")
    .supportedFamilies([.systemSmall])
  }
}
