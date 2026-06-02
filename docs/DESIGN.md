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

---

## 10. Backup & restore over WebDAV (Nextcloud) — 🟢 DONE (implemented)

Testers wanted multi-device data movement. The chosen first step is the
pragmatic, local-first-preserving one: **manual backup & restore to the user's
own Nextcloud over WebDAV** — cross-platform (plain HTTPS, no per-platform iCloud
/ Google native code), self-hosted, and it doubles as the backup users want. It
is **optional** (the app is fully usable with nothing configured) and **not**
automatic sync: no background upload, no merge/CRDT, no schema migration to UUIDs.
Those remain future work if concurrent multi-device editing is ever needed.

- **Single-writer, latest-snapshot model.** One fixed remote file,
  `Talea/talea-backup.sqlite3` under the user's files root. *Back up* overwrites
  it; *Restore* downloads it. No versioned history in v1 (a noted follow-up).
- **Backup** is a `VACUUM INTO` snapshot — a clean single file with no
  `-wal`/`-shm` sidecars — read into memory and `PUT`. **Restore** must **not**
  swap the live `sqlx` pool (every command holds `State<SqlitePool>`; swapping
  would mean refactoring them all). Instead it replaces table contents **in
  place, in one transaction** with `PRAGMA defer_foreign_keys = ON`: `ATTACH` the
  downloaded file, `DELETE` then `INSERT … SELECT *` across
  `account, category, entry, recurring_rule, rule_amount, rule_skip` (plus
  `sqlite_sequence`, so AUTOINCREMENT counters don't collide), `COMMIT`/`DETACH`.
  Any failure rolls back and leaves local data intact.
- **Same-version restore only.** Restore compares `MAX(version)` in each side's
  `_sqlx_migrations` and **refuses** a backup from a different schema version
  with a clear message, rather than risk a column mismatch. Cross-version restore
  is future work.
- **Credentials live in `nextcloud.json` in app-data — NOT in the database.**
  Deliberate: the password is therefore never part of an uploaded backup. The
  config getter returns address/username + a `configured` flag only; the password
  is never returned to the frontend and never logged. At rest it is plaintext
  protected by the same OS file-encryption baseline as the rest of the app's data
  (§9). Use a Nextcloud **app password** (app-scoped, revocable), not the login
  password. OS-keychain storage is a tracked follow-up (the `keyring` crate
  doesn't cover Android, so it isn't a clean cross-platform win today).
- **HTTPS only.** The WebDAV client (`src-tauri/src/webdav.rs`) rejects non-
  `https://` addresses and authenticates with HTTP Basic. TLS is `reqwest` +
  **rustls with the `ring` provider** (not the default `aws-lc-rs`), chosen so the
  iOS/Android cross-compile needs no OpenSSL and no C/cmake crypto toolchain; the
  provider is installed as rustls's process default at startup (`lib.rs`).
- **Network surface.** This re-introduces an *outbound* channel, but only to a
  user-supplied host, only on explicit action. There is no inbound server and no
  background networking; local-first is intact.

---

## 11. Summary accounts — 🟢 DONE (implemented)

Testers running several accounts wanted a combined overview of their total
budget. A **summary account** is a new, read-only account *kind* that aggregates
several **same-currency** normal accounts into one month view. This is a
deliberate, narrow amendment to the "no cross-account aggregation" stance of §2 /
§5: aggregation is allowed, but **only within a single currency**, so figures are
**summed, never converted** — the money rules hold.

- **Two kinds.** `AccountKind::{Normal, Summary}`. A normal account records its
  own entries/rules. A summary records nothing: it has no opening balance and no
  rules, and `core` fixes its opening balance to zero. Membership is a
  many-to-many relation (`account_member`), edited **on the summary account**.
- **Same-currency only.** A summary has a fixed currency; every member must match
  it (validated in the command layer, which `core` can't do — `core` only checks
  structural rules: a normal account has no members, a summary has a zero balance
  and distinct members). No nesting: members must be normal accounts.
- **Derived figures.** The summary's `MonthSummary` is the field-wise sum of its
  members' summaries (`core::combine_summaries`); its entry list, occurrences, and
  category stats are the members' rows concatenated and run through the existing
  per-account functions. Nothing is stored; everything is recomputed.
- **Read-only, enforced.** The UI hides the `+` button and renders rows
  non-interactively (tagged by source account with a stable colour). Behind that,
  every write command (`create_entry`, `update_entry`, `create_transfer`,
  `create_rule`, `update_rule`) rejects a summary target, and an account's kind is
  fixed after creation. A normal account that belongs to a summary can't change
  its currency (it would break the invariant) — remove it first.
- **Persistence.** Migration `0004` adds `account.kind` (defaulting existing rows
  to `normal`) and the `account_member` table (both FKs cascade, so deleting a
  member or a summary tidies the link). `account_member` joins the backup/restore
  `TABLES` set, so summaries round-trip.
- **Scope.** Covers the month view, the statistics screen, and the home-screen
  widget (a summary publishes the same abstract ring snapshot as any account).
