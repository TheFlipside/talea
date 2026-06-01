# Talea — Design Decisions

Canonical record of product/architecture decisions and the few details still
open. Status legend: 🔴 open · 🟡 leaning · 🟢 decided.

---

## 1. Budgeting model — 🟢 DECIDED: monthly cashflow ledger with carry-over

Talea is a **month-focused cashflow ledger**, not envelope budgeting and not
per-category limits.

- The unit of attention is **the current month and upcoming months**. The main
  screen shows, for the selected month, a top bar with **income, expenses, and
  the resulting available budget to end of month**, followed by the list of
  recorded entries. Swiping moves between months.
- **Available to end of month** = `carry-in + Σ income(month) − Σ expenses(month)`,
  where entries include both ad-hoc recordings **and** the expansion of recurring
  rules that apply to that month.
- **Carry-over is ON** (decided): a month's ending balance flows into the next
  month as `carry-in`. A surplus raises next month; an overspend reduces it.
  Months therefore form a **chain** per account.
- **Categories are descriptive**, used for classification and the stats screen —
  **not** funded envelopes and **not** spending limits. No per-category budget.
- A **"month" is a derived view** over dated entries, never a stored allocation
  row. The running balance is a cumulative sum, so no per-month table is needed.

### Carry-over chain & opening balance — 🟢 DECIDED

With carry-over on, the chain needs a starting point (confirmed):

- Each **Account** has an **`opening_balance`** (`Money`, default `0`) effective
  as of an **`anchor` month** (default: the account's creation month).
- Balance at end of month *M* = `opening_balance + Σ(entries dated ≤ end of M)`,
  income positive, expenses negative, recurring expanded. "Available this month"
  is that same running figure evaluated at month end.

---

## 2. Entities & relationships — 🟢 DECIDED (model), schema in §3

```
Account (1) ─── (N) Entry
Account (1) ─── (N) RecurringRule
Category (1) ─── (N) Entry           # category is optional on an entry
Category (1) ─── (N) RecurringRule   # categories are GLOBAL (shared by accounts)
```

- **Account** — `id`, `name`, `icon`, **`currency`** (fixed ISO 4217, per
  account), `opening_balance: Money`, `anchor` month. Multiple accounts; user
  switches between them. Data is scoped per account; **no cross-account
  aggregation or conversion** (see §6).
- **Category** — `id`, `label`, `icon` (a preset id **or** an emoji). **Global**:
  one list shared across all accounts, maintained on its own screen. Applies to
  both income and expense entries; the stats screen aggregates expenses by it.
- **Entry** — `id`, `account_id`, `amount: Money` (positive magnitude),
  `kind: Income | Expense`, `date`, `note: Option<String>`,
  `category_id: Option<Id>`. Full CRUD (add / edit / delete).
- **RecurringRule** — an entry template that is **dateless** but has
  `start_date`, `end: Never | Until(date)`, and a **frequency** (§4). It is
  *expanded* into virtual entries for each month it covers; expansions are
  computed, not stored (editing the rule re-derives them). Same fields as Entry
  otherwise (`account_id`, `amount`, `kind`, `note`, `category_id`).

`Month { year, month }` remains a value type used to window queries, not a row.

---

## 3. SQLite schema — 🟢 DONE (implemented in `src-tauri`)

`src-tauri/migrations/0001_init.sql` defines `STRICT` tables `account`,
`category`, `entry`, `recurring_rule`. Money and dates are **TEXT** (decimal
string; ISO-8601 `YYYY-MM-DD`); ids are `INTEGER PRIMARY KEY AUTOINCREMENT`;
enum-likes are TEXT with `CHECK`s matching the domain's serde tokens. FKs:
deleting an account cascades its entries/rules; deleting a category sets
referencing `category_id`s to NULL. Index `entry(account_id, date)` for
month-window queries. `core` never sees a connection — the `src-tauri` repository
maps rows↔domain through the validating constructors. Persistence uses `sqlx`
(bundled SQLite, WAL, `foreign_keys=ON`) with compile-time-checked queries
(committed `.sqlx` offline cache). The domain is reachable from the frontend only
through typed async Tauri commands.

---

## 4. Recurrence — 🟢 DECIDED: configurable intervals

Rules support **weekly / monthly / yearly**, with an **every-N** multiplier
(e.g. every 2 weeks). Anchored to `start_date`; an explicit `end` of `Never` or
`Until(date)`.

Edge cases to handle in the expansion logic (not the schema):

- **Month-end clamping:** a monthly rule starting on the 31st must resolve to the
  last valid day in shorter months (28/29/30).
- **Yearly Feb-29:** clamp to Feb-28 in non-leap years.
- Expansion is bounded by the queried month window; never enumerate to infinity.

---

## 5. Money & currency — 🟢 DECIDED

- All monetary values use `rust_decimal::Decimal`. **Floating point is forbidden**
  everywhere, including serialization and the frontend boundary, where money
  crosses as **strings**, never JSON numbers. See `core::money`.
- **Per-account currency:** each account stores a fixed ISO 4217 code. No FX
  conversion and no cross-account totals in v1. Display formats to the currency's
  minor units (rounding via `Money::round_dp`).

---

## 6. Home-screen widget — 🟢 DONE (implemented)

Shows only an **abstract ring + percentage** of budget health. **Absolute figures
never leave the core app** — only the ring fraction (0..1), a derived percent, an
overspent flag, and the account name are published to the OS shared storage the
widget reads. Actual numbers stay in-app, behind the optional biometric lock.

- **Configurable per widget:** each placed widget tracks one account (per-account
  currency means a widget can only ever reflect a single account — no
  cross-account aggregation). The current calendar month is shown.
- **Bridge:** an in-tree Tauri plugin `tauri-plugin-budgetwidget` exposes
  `publish_health(accounts)`. The frontend computes each account's fraction by
  reusing the in-app budget-ring view model (`ringView`), so money stays
  string-typed at the boundary; only the abstract snapshot crosses to native.
  Republished whenever data / accounts / ring-mode change while the app is open.
- **Android:** the widget (an `AppWidgetProvider` drawing the ring to a bitmap, a
  config `Activity` picking the account) lives in the plugin's Android library and
  merges into the app manifest. The plugin writes `SharedPreferences` and nudges
  the widgets to redraw.
- **iOS (17+):** the plugin writes an **App Group** (`group.com.luminaapps.talea`)
  and reloads timelines; the WidgetKit extension (`AppIntentConfiguration` account
  picker + SwiftUI ring) is a separate Xcode target whose sources live in
  `ios-widget/` and are added on macOS (see `docs/DEVELOPMENT.md`).
- **Limitation:** no background updater in v1 — between months while the app is
  closed the widget shows the last in-app snapshot until the app is next opened.

---

## 7. App lock — 🟢 DONE (implemented)

Access to the app is **optionally** gated behind device biometrics. Whole-app
lock (not per-screen), toggled in Settings and applied **on launch** (`LockGate`
wraps the app). Authentication uses `tauri-plugin-biometric`, a **mobile-only**
plugin (Android/iOS), with the device PIN/passcode allowed as a fallback.

- **Graceful degradation:** the plugin is not compiled into the desktop dev
  binary and is gated to mobile in capabilities (`capabilities/mobile.json`,
  `platforms: [android, iOS]`). Where biometrics are unavailable (desktop, or no
  enrolled biometrics) the app does **not** lock the user out — there is no way
  to authenticate, and desktop is a development target.
- **Toggle timing:** enabling/disabling the lock takes effect on the **next
  launch**, so flipping it on can't strand the user behind a prompt they cancel.
- **Lock-on-resume:** the lock re-engages when the app returns from the
  background, not only at cold start (`LockGate` listens for
  `visibilitychange`). A guard ignores the background/resume the native prompt
  itself can trigger (e.g. Android's `BiometricPrompt`) so it can't loop.

### System bar theming — 🟢 DONE (implemented)

The OS status/navigation bar icons can't be styled from the web layer, so an
in-tree Tauri plugin (`tauri-plugin-statusbar`, a committed workspace crate)
exposes a `set_dark` command that the frontend calls whenever the resolved theme
changes. It sets the bar icons to match **Talea's** theme — light icons in dark,
dark in light — so it's correct even when the device's own light/dark setting
differs (Android: `WindowInsetsControllerCompat`; iOS: the window's
`overrideUserInterfaceStyle`; desktop: no-op). The generated Android day/night
themes also set `windowLightStatusBar` as a pre-load default.

---

## 8. Domain validation & input limits — 🟢 DONE (implemented in `core`)

Implemented with the §2 types:

- **Validated constructors, private fields.** `Month::new` rejects months
  outside `1..=12`; typed IDs (`AccountId`/…/`RecurringRuleId`) wrap a private
  `u64`. Every validated type uses `#[serde(try_from = "…Repr")]` so malformed
  JSON (over IPC or storage) is rejected at deserialize, not silently accepted.
- **String length caps.** `note` ≤ `MAX_NOTE_LEN` (1000), labels/names ≤
  `MAX_LABEL_LEN` (200), counted in characters. Currency validated to a 3-letter
  ISO code.
- **Amount sign discipline.** `Entry`/`RecurringRule` amounts are positive
  magnitudes; the sign is derived from `EntryKind`. Zero/negative is rejected.

---

## 9. Security hardening backlog (tracked, non-blocking)

Accepted as known debt for the scaffold; revisit before a release:

- **CSP `style-src 'unsafe-inline'`.** Kept because Vite's pipeline may emit
  inline styles; `script-src` is already `'self'` (the XSS-critical one). Tighten
  to `'self'` via hashes/nonces once the production asset pipeline is settled and
  runtime-verified.
- **`core:event` emit scope.** `core:default` grants `allow-emit`/`allow-emit-to`.
  Harmless for one window; narrow to listen-only if a second webview is added.
- **`panic = "abort"`** (release profile) turns any handler panic into a hard
  crash. Mitigated by bounding IPC inputs and preferring `Money::checked_*`; keep
  that discipline as commands are added.
- **Ledger arithmetic uses `Money`'s `+`/`-`** (panic-on-overflow, by design,
  consistent with `money.rs`). Overflow is made unreachable in practice by two
  caps: `Month` years are bounded to `1..=9999` (so the carry-over walk length
  is bounded and date math can't panic), and entry/rule amounts are capped at one
  quadrillion — far below `Decimal::MAX`. If a stricter guarantee is ever wanted,
  convert the ledger functions to return `Result` with checked arithmetic. Note
  the ledger is O(history); the persistence layer may cache per-month aggregates.
- **The biometric app lock (§7) is a UI gate, not encryption.** The lock
  preference lives in `localStorage` and the lock deters casual access on a
  running device; it does not protect data against someone with filesystem
  access (root, ADB, an unencrypted device backup). It also auto-disengages
  where biometrics are unavailable (by design, so the user is never locked out).
  App-managed encryption (e.g. SQLCipher) plus OS-keystore-backed settings
  remains the stronger, tracked option before treating the lock as a
  confidentiality boundary. (The lock now re-engages on resume, not just at
  cold start — see §7.)
- **At-rest encryption — DECIDED: rely on the OS baseline for v1.** Both target
  platforms encrypt app-private storage at rest once the user has a device
  passcode/PIN, with **no app code and no entitlement**: iOS protects files at
  the `NSFileProtectionCompleteUntilFirstUserAuthentication` class by default,
  and modern Android encrypts internal storage (File-Based Encryption). The
  SQLite DB therefore already benefits on a secured device. The iOS **Data
  Protection** capability (`com.apple.developer.default-data-protection`,
  documented value `NSFileProtectionComplete`) is the opt-in to the *stronger*
  class where files are **sealed while the device is locked** — we deliberately
  **defer** it for v1: the resident `sqlx`/WAL connection stays open, and a
  sealed-when-locked file can raise `SQLITE_IOERR` if iOS suspends the app
  around device lock (or during background refresh). So v1 needs neither the
  capability nor the entitlement to get baseline at-rest encryption. (All of
  this only applies when the user has a device passcode/PIN set.) Raising specific files to `Complete` (set
  per-file, on-device-validated) or moving to SQLCipher is future work; the
  how-to and the entitlement snippet live in `docs/DEVELOPMENT.md`. Note the
  future widget's App Groups shared container (§6) must stay readable while
  locked, so it must **not** be `Complete`-protected.
- **Update commands trust the payload's `account_id`.** `update_account`/
  `update_entry`/`update_rule` locate the row by `id` and write the
  client-supplied `account_id`, so a crafted IPC call could in principle move a
  row between accounts. Harmless for a local single-user app (no privilege
  boundary), but if a wider IPC/sync surface is ever added, scope the `WHERE` to
  the owning account and stop writing `account_id` on update. Rule amount history
  is already bounded (`MAX_AMOUNT_SEGMENTS`) alongside the note/amount caps.
