# iOS home-screen widget (`TaleaWidget`)

These are the sources for the iOS WidgetKit extension that renders the abstract
budget-health ring (see `docs/DESIGN.md` §6). A WidgetKit extension is a
**separate Xcode target**, so — unlike `tauri-plugin-budgetwidget` — it can't be
contributed by a Tauri plugin and isn't built by `cargo tauri ios build`. It must
be added to the generated Xcode project on macOS. These files are the source of
truth; re-apply after any `cargo tauri ios init` (which regenerates
`src-tauri/gen/apple`).

Targets **iOS 17+** (uses `AppIntentConfiguration` for the per-widget account
picker and `containerBackground`).

## Files

- `TaleaWidget/SharedHealth.swift` — reads the snapshot the app publishes to the
  App Group (`group.com.luminaapps.talea`). Carries no money, only the ring
  fraction.
- `TaleaWidget/SelectAccountIntent.swift` — the configuration intent + account
  picker (names sourced from the published snapshot).
- `TaleaWidget/TaleaWidget.swift` — the timeline provider + SwiftUI ring view.
- `TaleaWidget/TaleaWidgetBundle.swift` — the `@main` widget bundle.
- `TaleaWidget/Info.plist`, `TaleaWidget/TaleaWidget.entitlements` — extension
  Info.plist and the App Group entitlement.

## One-time setup in Xcode (on macOS)

1. `cargo tauri ios init` (if not already), then open `src-tauri/gen/apple/*.xcodeproj`.
2. **App Group**: enable the App Groups capability on **both** the app target and
   (in the next step) the widget target, with the group
   `group.com.luminaapps.talea`. Register the same group on the App ID in the
   Apple Developer portal.
3. **Add the widget target**: File → New → Target → *Widget Extension*, name it
   `TaleaWidget`, uncheck "Include Configuration Intent" (we provide our own
   AppIntent). Delete the stub files Xcode generates and add the files from
   `TaleaWidget/` here to the new target. Set the target's Info.plist to this
   `Info.plist` and its Code Signing Entitlements to `TaleaWidget.entitlements`.
4. Set the widget target's **Minimum Deployment** to iOS 17.
5. Build/run on a device or simulator; long-press the widget → **Edit Widget** to
   pick an account.

## Data-protection note

The App Group container the widget reads must stay readable while the device is
locked, so it must **not** be `NSFileProtectionComplete`-protected (see
`docs/DESIGN.md` §9). The default protection class is fine.
