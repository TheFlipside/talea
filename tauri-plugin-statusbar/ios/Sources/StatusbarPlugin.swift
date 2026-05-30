import SwiftRs
import Tauri
import UIKit
import WebKit

struct SetDarkArgs: Decodable {
  let dark: Bool
}

class StatusbarPlugin: Plugin {
  // Match the system status bar (and any native UI) to the app theme by forcing
  // the window's interface style. With the status bar's default style this gives
  // light (white) icons in dark, dark icons in light.
  @objc public func setDark(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SetDarkArgs.self)
    // Resolve from inside the main-thread block, after the change is applied.
    DispatchQueue.main.async {
      let style: UIUserInterfaceStyle = args.dark ? .dark : .light
      for scene in UIApplication.shared.connectedScenes {
        guard let windowScene = scene as? UIWindowScene else { continue }
        for window in windowScene.windows {
          window.overrideUserInterfaceStyle = style
        }
      }
      invoke.resolve()
    }
  }
}

@_cdecl("init_plugin_statusbar")
func initPlugin() -> Plugin {
  return StatusbarPlugin()
}
