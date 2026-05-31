import SwiftRs
import Tauri
import UIKit
import WidgetKit

// Carries no money — only an abstract ring fraction per account.
struct AccountHealth: Codable {
  let id: String
  let name: String
  let fraction: Double
  let overspent: Bool
}

struct PublishArgs: Decodable {
  let accounts: [AccountHealth]
}

class BudgetWidgetPlugin: Plugin {
  // Shared with the TaleaWidget extension target (App Groups capability).
  static let appGroup = "group.com.luminaapps.talea"
  static let key = "accounts"

  @objc public func publishHealth(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PublishArgs.self)
    // A nil suite means the App Group entitlement is missing/misconfigured —
    // surface it rather than silently dropping the update.
    guard let defaults = UserDefaults(suiteName: BudgetWidgetPlugin.appGroup) else {
      invoke.reject("App Group \(BudgetWidgetPlugin.appGroup) is unavailable")
      return
    }
    do {
      let data = try JSONEncoder().encode(args.accounts)
      defaults.set(data, forKey: BudgetWidgetPlugin.key)
    } catch {
      invoke.reject("Failed to encode widget data: \(error.localizedDescription)")
      return
    }
    if #available(iOS 14.0, *) {
      WidgetCenter.shared.reloadAllTimelines()
    }
    invoke.resolve()
  }
}

@_cdecl("init_plugin_budgetwidget")
func initPlugin() -> Plugin {
  return BudgetWidgetPlugin()
}
