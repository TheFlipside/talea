# iOS home-screen widget (`TaleaWidget`)

These are the sources for the iOS WidgetKit extension that renders the abstract
budget-health ring (see `docs/DESIGN.md` §6). A WidgetKit extension is a
**separate Xcode target**, so — unlike `tauri-plugin-budgetwidget` — it can't be
contributed by a Tauri plugin.

`src-tauri/gen/apple` is regenerated from `project.yml` (XcodeGen) on every
`cargo tauri ios dev/build`, so the target **must be declared in `project.yml`,
not the Xcode UI** (UI edits are wiped). `scripts/configure_ios_project.py` (run
by `just ios-init`) does exactly that — it adds the `TaleaWidget` app-extension
target referencing the files here, embeds it in the app, and regenerates the
project. So you **don't** add anything in Xcode by hand; just run `just ios-init`.

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

## Setup (on macOS)

1. Register the App Group `group.com.luminaapps.talea` in the Apple Developer
   portal (see `docs/DEVELOPMENT.md` → iOS → One-time setup). Automatic signing
   handles the rest.
2. `just ios-init` — wires the target into `project.yml` and regenerates the
   Xcode project. (Needs `pip install pyyaml` and `brew install xcodegen`.)
3. `just ios-dev` / `just ios-release` build the app with the widget embedded.
   On the device, long-press the widget → **Edit Widget** to pick an account.

## Data-protection note

The App Group container the widget reads must stay readable while the device is
locked, so it must **not** be `NSFileProtectionComplete`-protected (see
`docs/DESIGN.md` §9). The default protection class is fine.
