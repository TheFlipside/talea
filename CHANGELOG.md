# Changelog

All notable changes to this project are documented in this file.

## Unreleased

## 1.4.0 - 2026-06-02

### Added

- **Summary accounts** — a read-only account type that aggregates several
  **same-currency** normal accounts into one combined month view. Pick *Summary*
  when creating an account and choose its member accounts; selecting it shows the
  combined income / expenses / available and the merged entry list (each row
  colour-tagged and labelled by its source account), the combined
  category-expense statistics, and works as a widget target. It takes no records
  of its own: the `+` button is hidden and every write command rejects it.
  Translated into all 12 locales. (Stays faithful to the money rules — members
  must share a currency, so figures are summed, never converted; see DESIGN §11.)

### Changed

- The account form gained an account-type selector and (for a summary) a member
  checklist; the account switcher marks summary accounts with a badge.

## 1.3.0 - 2026-06-01

### Added

- **Backup & restore to your own Nextcloud** (manual, over WebDAV). A new
  *Backup & sync* section in Settings takes a Nextcloud address, username, and
  **app password**, with *Test connection*, *Back up now*, and *Restore* (guarded
  by a confirmation, since it replaces all local data). Backups are a clean
  `VACUUM INTO` snapshot uploaded to `Talea/talea-backup.sqlite3`; restore is an
  in-place, single-transaction table replace (no pool swap) that refuses a backup
  from a different schema version rather than risk a mismatch. Stays local-first:
  no automatic sync, no merge, and the app is fully usable with nothing
  configured. Translated into all 12 locales.
- Credentials are stored in `nextcloud.json` in the app-data directory —
  deliberately **outside** the database, so the password is never part of an
  uploaded backup — and the password is never returned to the frontend or logged.

### Changed

- Networking uses `reqwest` 0.13 with **rustls + the `ring` provider** (not the
  default `aws-lc-rs`), keeping the iOS/Android cross-compile free of a C/cmake
  TLS toolchain.

## 1.2.0 - 2026-06-01

### Added

- Translations for 11 more languages: German, Spanish, French, Italian,
  Portuguese, Dutch, Japanese, Simplified Chinese, Russian, Polish, Turkish —
  selectable in Settings and auto-detected from the device locale. A test asserts
  every locale carries all English keys. (Machine-translated; pending a
  native-speaker pass on domain terms.)
- Biometric lock now **re-engages on resume**, not just at cold start: returning
  from the background requires authenticating again (`LockGate` listens for
  `visibilitychange`, with a guard so the native prompt can't trigger a loop).

### Fixed

- Amount fields now accept a decimal **comma** (e.g. `0,99`): input is normalized
  to a dot before validation/submit, and the example hints show each locale's
  separator. Previously comma-locale users were rejected with a dot-only example.
- Statistics screen: an empty month collapsed the swipe area so you couldn't
  page out of it; `.stats-screen` now fills the height, keeping swipe navigation
  working when there's no data.
- Surface widget-publish failures (`console.warn`) so a misconfigured iOS App
  Group is diagnosable; documented the App Group registration as the fix for an
  empty widget / account picker.

## 1.1.0 - 2026-06-01

### Added

- iOS home-screen widget now ships in the build. The WidgetKit extension can't be
  a Tauri plugin and the Xcode project is regenerated from `project.yml`, so
  `scripts/configure_ios_project.py` (run by `just ios-init`) declares the
  `TaleaWidget` app-extension target — sources from `ios-widget/`, App Group
  `group.com.luminaapps.talea`, embedded in the app — and regenerates the project
  with `xcodegen`. Requires the App Group registered in the Apple Developer portal
  (documented in `docs/DEVELOPMENT.md`). The extension's `Info.plist` declares
  `CFBundleExecutable`, `CFBundlePackageType` `XPC!`, and app-matched version keys
  so App Store validation accepts the bundle.
- Turnkey iOS workflow: `just ios-init` (project + branded icons), `just ios-dev`
  (live device/simulator), and `just ios-release` (signed App Store IPA via
  `--export-method app-store-connect`). The Apple development team is set once via
  `tauri.conf.json` `bundle.iOS.developmentTeam` or `APPLE_DEVELOPMENT_TEAM`, so
  signing no longer needs the Xcode team picker. Documented end-to-end in
  `docs/DEVELOPMENT.md`, including the distribution-cert prerequisite, signature
  verification, the `errSecInternalComponent` keychain fix, that a bare
  `cargo tauri ios build` exports a *development*-signed IPA (use `just
  ios-release`), and the Organizer fallback + Transporter upload.
- Set the app `category` ("Finance") in `tauri.conf.json` so iOS gets a valid
  `LSApplicationCategoryType` (required for App Store).

- Home-screen widget (Android + iOS): an **abstract budget-ring + percentage**
  for a chosen account, **configurable per widget**. Only the ring fraction,
  percent, overspent flag and account name cross to the OS — never any amount —
  so figures stay in-app behind the biometric lock (DESIGN.md §6). A new in-tree
  Tauri plugin `tauri-plugin-budgetwidget` publishes the abstract snapshot
  (computed by reusing the in-app `ringView`); Android ships the widget in the
  plugin's library (manifest-merged `AppWidgetProvider` + account-picker config
  activity, ring drawn to a bitmap); iOS ships a WidgetKit extension (sources in
  `ios-widget/`, App Group `group.com.luminaapps.talea`, added in Xcode on macOS).
  The widget is **reconfigurable** (Android 12+): long-press → reconfigure to
  change the tracked account without removing and re-adding it.

### Fixed

- iOS biometric app lock never engaged (toggled on, relaunch, app opened
  unguarded): the iOS Info.plist had no `NSFaceIDUsageDescription`, so on a Face
  ID device `LAContext.canEvaluatePolicy` reports biometrics unavailable and
  `LockGate`'s graceful-degradation path unlocked immediately (Android needs no
  such string, hence it worked there). `configure_ios_project.py` now injects the
  key. iOS-only; no frontend change.
- iOS App Store rejected the build because the Rust `libapp.a` was copied into the
  app bundle (XcodeGen treats the `Externals` source dir's `.a` as a resource —
  "standalone library not permitted"). `configure_ios_project.py` now marks that
  source `buildPhase: none`, so it's linked (via the `framework: libapp.a`
  dependency) but not bundled.
- Android launcher icon shipped as the **default Tauri logo** instead of Talea's:
  `cargo tauri android init` scaffolds the stock icon, and nothing reapplied the
  branded one. The `android-init` recipe now runs `cargo tauri icon` against a new
  `src-tauri/icons/icon-manifest.json` (adaptive icon = ring/calendar on the dark
  `#122E38` tile; also fills the iOS icons' transparent corners).
- `tauri-plugin-statusbar` ProGuard keep-rule still referenced the pre-rebrand
  package; updated to `com.luminaapps.talea.statusbar` so R8 can't strip the
  reflectively-loaded plugin class in release builds.
- macOS/iOS build failed compiling the Apple-only `dispatch2` crate
  (`recursion limit reached while expanding __bitflags_flag_name`): a `bitflags`
  2.12.0 regression recurses per flag attribute, overrunning the default limit on
  `dispatch2`'s heavily-documented flags. Pinned `bitflags` to 2.9.1 in
  `Cargo.lock` (the 2.x line has no such macro).
- A fresh clone's first `just dev` (or `build`/`test`/`android-*`) failed because
  the frontend deps weren't installed yet. Those recipes now depend on a
  `_ensure-frontend` guard that runs `npm install` only when `node_modules` is
  missing.
- iOS launcher icon shipped as the default Tauri logo for the same reason as
  Android (`ios init` scaffolds the stock icon). `just ios-init` now reapplies the
  branded icon via the icon manifest.
- App Store upload rejected the icon ("Invalid large app icon … can't be
  transparent or contain an alpha channel"): `cargo tauri icon` emits RGBA for the
  iOS set. Added `scripts/flatten_ios_icons.py` (run by `just ios-init`) to
  composite the iOS icons over the tile colour and rewrite them as opaque RGB;
  desktop/Android/favicon keep their transparency.
- Documented the real release-APK path (`…-release-unsigned.apk`) and the
  zipalign → apksigner signing flow in `docs/DEVELOPMENT.md` (the previous path
  assumed an auto-signed release).

## 1.0.0 - 2026-05-31

First release prepared for the app stores; bundle identifier finalized to
`com.luminaapps.talea`.

### Added

- Initial project scaffold: Cargo workspace with `core/`, `src-tauri/`, and
  `frontend/`, plus `README.md` and architecture/decisions in `CLAUDE.md`.
- `core` crate: `Money` newtype over `rust_decimal::Decimal` (no floating
  point), with construction/formatting helpers and unit tests.
- `core` domain model implementing the decided monthly cashflow ledger:
  `Account` (per-account `Currency`, opening balance + anchor month),
  global `Category`/`CategoryIcon`, `Entry`/`EntryKind`, and `RecurringRule`
  with configurable `Frequency` (weekly/monthly/yearly + every-N) and
  `RuleEnd`. Recurrence expansion clamps month-end/leap-day dates without
  drift; the `ledger` module computes carry-over `MonthSummary`s
  (`month_summary`/`summaries_for_range`/`balance_at_end_of`). Validated
  constructors with `serde(try_from)` so malformed input is rejected at the
  boundary; dates cross as ISO `YYYY-MM-DD` strings, money as strings.
  Added the `time` dependency (pure date math). 54 unit tests.
- Minimal Tauri 2 + React/TypeScript (Vite) smoke screen bridging `core` to the
  frontend via a typed `invoke` command.
- Quality gates pass clean from the first commit: `cargo clippy -W
  clippy::pedantic -D warnings`, `cargo fmt --check`, and
  `eslint --max-warnings=0`.
- SQLite persistence in `src-tauri` via `sqlx` (bundled SQLite): `STRICT` schema
  (`account`/`category`/`entry`/`recurring_rule`) with FK cascade/set-null,
  migrations run on startup, a WAL connection pool in Tauri state, and a
  repository mapping rows↔domain through the validating constructors (a failing
  read is reported as corruption, not user error). Money/dates stored as TEXT.
- Typed async Tauri command surface: CRUD for accounts, categories, entries, and
  recurring rules, plus `month_summary`/`summaries_for_range` that load an
  account and call the pure `core` ledger. Errors cross as `{code, message}`;
  internal details are logged, not exposed.
- Compile-time-checked SQL: `sqlx::query!` with a committed `.sqlx` offline cache
  and `SQLX_OFFLINE=true` (`.cargo/config.toml`), so a fresh checkout builds with
  no database. `src-tauri` integration tests (temp DB) cover round-trips, FK
  behavior, the ledger query, corruption detection, and migration idempotency.
- Frontend core loop (Phase 1): a typed `invoke` layer + TanStack Query hooks, an
  account onboarding/switcher, and a month screen showing income / expenses /
  available-to-end-of-month with a hand-rolled SVG budget ring, the month's entry
  list, add/edit/delete entry forms, and prev/next + swipe month navigation. Money
  stays a string end-to-end (parsed only for display); dates use native ISO date
  inputs. Vitest unit tests for the pure money/month/entry helpers. (Category,
  recurring-rule, and stats screens are the next phase.)
- First-run now auto-creates a default account (currency from the system locale,
  fallback USD); the create-account form is reachable via the account switcher.
  Custom styled dropdown (`Select`) and calendar date picker replace the native
  `<select>`/`<input type=date>` so they match the theme and dismiss on
  outside-click; currency is chosen from a list showing symbols. Client-side
  input validation surfaces friendly inline errors instead of opaque backend
  deserialization failures. Documented resetting the local dev database.
- `LICENSE` (MIT) and a `justfile` of common tasks (dev/build/test/lint/gate,
  sqlx-prepare, reset-db) including a `crap` recipe for CRAP coverage diagnosis
  (`cargo llvm-cov` + `cargo crap`).
- Phase-2 foundation: **internationalization** (react-i18next; all UI strings in
  an English catalog, ready for more languages), a **Settings screen** (theme
  light/dark/system with a light palette, language, and what the budget ring
  shows — spent vs remaining), a **navigation restructure** (a settings cog
  replaces the top-bar "+ Account"; an icon nav bar for the manager screens),
  and a **Manage Accounts** screen (add / edit / delete, with a delete
  confirmation that warns the cascade). The category / recurring / stats nav
  icons route to "coming soon" placeholders.
- Category manager: add / edit / delete global categories with an emoji picker,
  prefilled on first run with a dozen common categories (labels localized).
  Categories are selectable on entries and shown (icon + label) in the month
  list. Deleting a category keeps the entries and just clears their category.
- Statistics screen: a breakdown of the selected month's expenses by category —
  a hand-rolled SVG pie chart (each slice labelled with the category emoji + its
  share) above proportional bars, with slice and bar sharing one color per
  category. Month-navigable like the month screen, and the account switcher is
  available here too for quick per-account comparison. A new pure
  `core::expenses_by_category` aggregates a month's expenses (ad-hoc entries +
  recurring expansions) grouped by category, with its own `expenses_by_category`
  Tauri command. **Uncategorized expenses are folded into a single "Other"
  slice** (the `null` category bucket); the seeded defaults no longer include a
  real "Other" category, so it can't appear twice.

- Recurring-rule manager: add / edit / delete recurring income/expense rules on
  their own screen (cadence, start, end, category, note), per account. A rule's
  occurrences now also appear as read-only 🔁 rows in the month list (previously
  they only affected the totals). The account switcher is available on this
  screen for quick per-account context, as on the month and stats screens.
- **Effective-dated rule amounts.** A rule carries an *amount history* (a new
  `core::AmountSegment`): the amount in effect is resolved per occurrence date,
  so a change can apply **from a chosen month onward without rewriting the
  past** — essential because the ledger chains carry-over and a retroactive
  change would alter historical balances. Editing an amount offers "this month
  onward" (adds a breakpoint) or "all months" (collapses to a single base). The
  base amount stays on `recurring_rule`; later breakpoints live in a new
  `rule_amount` child table (migration `0002`). `VirtualEntry` now carries its
  `rule_id`, and a `month_occurrences` command expands a month's occurrences.

- Per-occurrence overrides for recurring rules. Tapping a 🔁 occurrence in the
  month list offers **remove just this one** (a "skip" — the expansion omits that
  date) or **edit just this one** (detaches it into a normal standalone entry
  with the occurrence's values, then opens the editor; later rule changes no
  longer affect it). Skips are stored in a new `rule_skip` child table (migration
  `0003`) and attached to rules on load, so `core` expansion — and therefore the
  totals, stats, and month list — all honour them with no signature changes.
  Adds `skip_occurrence` / `detach_occurrence` commands (detach is one
  transaction) and `RecurringRule::with_skips`.

- Account transfers: when adding an entry and more than one same-currency
  account exists, a toggle offers to mirror it onto another account as the
  opposite kind ("also record as income/expense on …"), keeping the same
  amount, date, note, and category on both sides. A new atomic
  `create_transfer` command writes both entries in one transaction (no currency
  conversion — only same-currency accounts are offered). The two entries are
  independent afterward. Adds `EntryKind::opposite`.
- A **Home** button is now the leftmost icon in the navigation bar, switching
  back to the month view.

- Optional **biometric app lock**: a Settings toggle ("Require biometric
  unlock") that gates the app on launch behind device biometrics (or the device
  PIN as fallback), via the mobile-only `tauri-plugin-biometric`. A `LockGate`
  wraps the app; the lock applies from the next launch. Where biometrics are
  unavailable (the desktop dev build, or no enrolled biometrics) the app does
  not lock — the plugin is gated to mobile in `capabilities/mobile.json` and not
  compiled into the desktop binary.

- Native status-bar theming: a small in-tree Tauri plugin
  (`tauri-plugin-statusbar`) sets the OS status/navigation bar icon appearance
  to match the **app's** theme on Android and iOS — light icons in dark mode,
  dark icons in light mode — driven from the theme so it's correct regardless of
  the device's own light/dark setting (a no-op on desktop). The frontend calls
  it whenever the resolved theme changes.

- Tap the month/year label in the month navigation to jump straight back to the
  current month; the current month is shown in bold so it's clear at a glance
  when the view has drifted away from "now".

- App logo: the budget ring now carries a small calendar glyph in its centre.
  Regenerated all platform icons (desktop, Android, iOS, store logos) from a
  single `src-tauri/icons/app-icon.svg` source, and matched the in-app favicon.

### Changed

- Bundle identifier changed from the `app.talea.budget` development placeholder
  to the published reverse-DNS id `com.luminaapps.talea` (the Android
  `applicationId` / iOS bundle id, and the on-device app-data directory). The
  in-tree `tauri-plugin-statusbar` Android package was renamed to match
  (`com.luminaapps.talea.statusbar`). Regenerate the native `gen/` projects
  (`cargo tauri android init` / `ios init`) so the change takes effect locally.
- The date picker's calendar now renders in normal flow rather than as an
  absolute popup, so the entry/rule modal grows to use the available viewport
  height instead of clipping the calendar and forcing a scroll.

### Fixed

- Respect mobile safe areas: the layout now honours `env(safe-area-inset-*)`
  (via `viewport-fit=cover`) so the header clears the status bar / camera notch
  (the settings cog was previously unreachable under the status bar), the FAB
  and scrollable content clear the bottom gesture bar, and modals stay within
  the safe area.
- `index.html` favicon declared `type="image/png"` for an SVG asset; corrected
  to `image/svg+xml`.

### Security

- Set an explicit Content-Security-Policy (`script-src 'self'`, scoped
  `img-src`/`connect-src`) instead of `null`.
- Scoped the window capability to least privilege: dropped the unused
  `opener:default` and explicitly denied `core:image:from-path`/`from-bytes`.
- Bounded untrusted IPC string input in the `smoke_check` command (UTF-8-safe
  char cap) and capped the snippet echoed in `MoneyError::Parse`.
- Runtime-validate the `invoke` payload shape in the frontend so a Rust/TS
  contract drift surfaces as an error instead of silent blanks.
- Documented remaining hardening backlog (CSP `style-src`, event-emit scope,
  domain input validation) in `docs/DESIGN.md` §5–6.

### Changed

- Moved `rust_decimal_macros` to `core` dev-dependencies (test-only).
- Added `frontend` `typecheck` script and a window minimum size.

### Removed

- `tauri-plugin-opener` and its frontend package (unused in the scaffold).

### 0.1.0 - 1970-01-01

### Added

- Changes that add new functionality or features.

### Fixed

- Resolved bugs and issues.

### Security

- Resolved security related issues.

### Removed

- Features or functionalities that got removed.

