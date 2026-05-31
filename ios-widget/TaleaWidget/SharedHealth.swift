import Foundation

/// One account's abstract budget health for the current month. Carries no money
/// — only the ring fraction the widget draws. Mirrors the struct the app writes
/// in `tauri-plugin-budgetwidget`.
struct AccountHealth: Codable, Identifiable {
  let id: String
  let name: String
  let fraction: Double
  let overspent: Bool
}

/// Reads the snapshot the app publishes into the shared App Group container.
enum SharedHealth {
  /// Must match the App Group enabled on both the app and this extension, and
  /// the id used by `tauri-plugin-budgetwidget`.
  static let appGroup = "group.com.luminaapps.talea"
  static let key = "accounts"

  static func load() -> [AccountHealth] {
    guard let defaults = UserDefaults(suiteName: appGroup),
      let data = defaults.data(forKey: key),
      let decoded = try? JSONDecoder().decode([AccountHealth].self, from: data)
    else {
      return []
    }
    return decoded
  }

  static func health(for id: String?) -> AccountHealth? {
    guard let id = id else { return nil }
    return load().first { $0.id == id }
  }
}
