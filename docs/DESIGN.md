# Talea — Design Decisions

Canonical record of product/architecture decisions and the few details still
open. Status legend: 🔴 open · 🟡 leaning · 🟢 decided.

### Stack & key decisions (at a glance)

| Decision        | Choice                          | Rationale                                                                  |
| --------------- | ------------------------------- | -------------------------------------------------------------------------- |
| Shell           | Tauri 2.x                       | One Rust core, native Android + iOS + desktop, small footprint.            |
| Frontend        | React + TypeScript (Vite)       | Familiar, fast HMR, typed boundary to the shell.                           |
| Persistence     | local SQLite via `sqlx`         | Local-first, embeddable, queryable, no server.                             |
| Money           | `rust_decimal` — **never f64**  | Exact base-10 arithmetic; floating point is forbidden for monetary values. |
| Domain location | pure `core` crate               | Logic stays IO-free and unit-tested, isolated from the shell.              |

The sections below record the product and architecture decisions in full.

---

## 1. Budgeting model — 🟢 DECIDED: monthly cashflow ledger with carry-over

Talea is a **month-focused cashflow ledger**, not envelope budgeting.

- The unit of attention is **the current month and upcoming months**. The main
  screen shows, for the selected month, a top bar with **income, expenses, and
  the resulting available budget to end of month**, followed by the list of
  recorded entries. Swiping moves between months. In the current month the bar
  also leads with the **balance today** (§13).
- **Available to end of month** = `carry-in + Σ income(month) − Σ expenses(month)`,
  where entries include both ad-hoc recordings **and** the expansion of recurring
  rules that apply to that month.
- **Carry-over is ON** (decided): a month's ending balance flows into the next
  month as `carry-in`. A surplus raises next month; an overspend reduces it.
  Months therefore form a **chain** per account.
- **Categories are descriptive**, used for classification and the stats screen —
  **not** funded envelopes. A category may carry a **budget** (§12): a planned
  monthly amount that the month's spending in it consumes. That is a *plan*, not
  a limit — nothing is ever refused for exceeding it, and an unspent budget
  expires into ordinary carry-over rather than rolling forward.
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
enum-likes are TEXT with `CHECK`s matching the domain's serde tokens. Later
migrations add `rule_amount` (0002), `rule_skip` (0003), `account.kind` +
`account_member` (0004), and `budget` (0005). FKs: deleting an account cascades
its entries/rules/budgets; deleting a category sets referencing `category_id`s to
NULL **except** on `budget`, which cascades (§12). Index `entry(account_id, date)`
for month-window queries. `core` never sees a connection — the `src-tauri` repository
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

Since §12 the ring follows **`free`** (`available − reserved`) rather than
`available`, so a category budget's unspent remainder counts as committed. With
no budgets `reserved` is zero and the ring is unchanged. Nothing about the
boundary changes: `ringView` still emits only the fraction and the overspent
flag, so no money crosses it.

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
  Since the launch gate reads it from storage, the switch only moves once
  storage has accepted that write: shown as on but never stored, it would
  promise a lock the next launch doesn't keep.
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
- **Anchor floor.** An account's anchor month can't precede 1969
  (`MIN_ANCHOR_YEAR`). Every month view walks each month from the anchor over
  all of the account's rows, so an anchor centuries back would make every view
  crawl. The app only ever anchors an account at the month it is created, by
  the device clock, so only a crafted account starts earlier (1969, not 1970:
  a clock reset to the Unix epoch reads December 1969 west of Greenwich).

---

## 9. Security hardening backlog (tracked, non-blocking)

Accepted as known debt for the scaffold; revisit before a release:

- **CSP `style-src 'unsafe-inline'`.** Kept because Vite's pipeline may emit
  inline styles; `script-src` is already `'self'` (the XSS-critical one). Tighten
  to `'self'` via hashes/nonces once the production asset pipeline is settled and
  runtime-verified.
- **The webview stays on the app's own pages.** The CSP governs what a page
  loads and connects to, not where it goes, so a script that read the ledger
  could have carried it off by navigating to a server of its own, the data in
  the URL, or to a scheme another app handles (`mailto:`, `intent:`). An in-tree
  plugin (`navigation.rs`) now cancels every top-level navigation off the app's
  origin: on each platform the one Tauri serves it from (`tauri://localhost`, or
  `http(s)://tauri.localhost` on Android and Windows, mirrored from Tauri's own
  choice, so a Tauri upgrade must re-check them), and in a dev build the dev
  server. Frames are held by the CSP (`default-src 'self'`), which matters on
  Windows, where WebView2 doesn't report a frame's navigation to the guard. The
  CSP also adds `form-action 'none'` and `base-uri 'none'`; `img-src` no longer
  names the asset protocol, which nothing uses; and `connect-src` names
  `http://ipc.localhost` only on Windows, whose IPC runs over it, and Android,
  which fetches channel data there (`tauri.windows.conf.json`,
  `tauri.android.conf.json`, overriding just that directive; a test holds them
  to it). Elsewhere that host, like the other platforms' app origin, is an
  ordinary name a resolver might send anywhere. `window.open` opens no window on
  any platform (no handler is set, and each engine's default declines) and stays
  in the same, guarded view on Android. Left open, with no switch Tauri or the
  CSP offers: a script can still carry data out in DNS lookups (`<link
  rel="dns-prefetch">`, `preconnect`) and, where the engine ships WebRTC
  (Android, iOS, Windows), in the credentials of a TURN server it asks for. The
  defence there is keeping untrusted script out of the bundle. Also known: a URL
  the runtime can't parse is let through rather than checked (an upstream choice
  in `tauri-runtime-wry`; the engines parse URLs the same way, so none is known
  to slip by); a dev build serves the frontend from the dev server, without the
  CSP; and on Android the guard rests on WebView's `shouldOverrideUrlLoading`,
  to be checked on a device for `location` changes, a meta refresh,
  `window.open`, and `mailto:` or `intent:` links clicked from script.
- **`core:event` emit scope.** `core:default` grants `allow-emit`/`allow-emit-to`.
  Harmless for one window; narrow to listen-only if a second webview is added.
- **`panic = "abort"`** (release profile) turns any handler panic into a hard
  crash. Mitigated by bounding IPC inputs and preferring `Money::checked_*`; keep
  that discipline as commands are added.
- **Ledger arithmetic uses `Money`'s `+`/`-`** (panic-on-overflow, by design,
  consistent with `money.rs`). Overflow is made unreachable in practice by
  capping every term: `Month` years are bounded to `1..=9999` (so the carry-over
  walk length is bounded and date math can't panic; dates arriving over IPC are
  held to the same range), entry/rule amounts are capped at one quadrillion —
  far below `Decimal::MAX` — and so is the magnitude of an account's opening
  balance, which seeds every running balance. That last cap was missing until
  the §13 security audit: an absurd opening balance, typed in or restored from a
  tampered backup, overflowed and, with `panic = "abort"`, crashed the app. A
  stored value beyond the cap now reads as a corrupt row, and restore refuses a
  backup holding one (§10). With every term capped, an overflow takes some 8e13
  maximal terms, and rows can't supply that many: a recurring rule expands to
  at most about 520,000 (weekly, across all 9,999 years), so it would take over
  10^8 rules. Restore, the only bulk writer, holds a backup to 1,000 rules and
  250,000 entries (§10), some 10^5 times short of that. A sum across ledgers in the command layer
  (`balance_as_of`'s summary total) uses `checked_add` on top. If a stricter
  guarantee is ever wanted, convert the ledger functions to return `Result` with
  checked arithmetic. Note the ledger is O(history); the persistence layer may
  cache per-month aggregates.
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
  `update_entry`/`update_rule`/`update_budget` locate the row by `id` and write
  the client-supplied `account_id`, so a crafted IPC call could in principle move
  a row between accounts. (Each still re-runs its cross-entity validation, so the
  move can't break an invariant — a budget can't land on a summary account or
  duplicate an existing category budget — only re-parent to another normal one.) Harmless for a local single-user app (no privilege
  boundary), but if a wider IPC/sync surface is ever added, scope the `WHERE` to
  the owning account and stop writing `account_id` on update. Rule amount history
  is already bounded (`MAX_AMOUNT_SEGMENTS`) alongside the note/amount caps.
- **No log backend is installed.** Every `log::warn!`/`log::error!`, the
  restore refusal diagnostics included, is a no-op: nothing initialises a
  logger. Adding one (e.g. `tauri-plugin-log`) needs a redaction policy first,
  since some messages quote stored values (each bounded to 64 characters).
- **Restore holds the write lock while it validates.** The read-back (§10) runs
  inside the restore's transaction, so a write issued meanwhile waits on the
  5 s busy timeout: past about a million entries, that can fail. Accepted:
  validating what is about to be committed is the point, and validating the
  file beforehand would read through its indexes, which nothing verifies.
- **The SQLite heap cap narrows an out-of-memory failure, not removes it.** A
  crafted backup whose statistics nearly fill the cap while it is attached can
  make another connection's query fail to copy a row, and sqlx `expect`s that
  copy, so the app aborts. A single oversized request fails without taking any
  memory, so this needs a file sized to fill the cap without overrunning it,
  and a query landing in that moment. Closing it needs sqlx to report that
  failure rather than panic.
- **An unreadable account row blanks the app.** Accounts load as one list, so
  a single row the loaders refuse (only a tampered database, or one restored
  by an older build from a tampered backup, holds one) fails the whole list,
  and the app shows only an error, leaving Settings and Restore out of reach.
  Fix: let Settings open without the account list, so a good backup can be
  restored.
- **Restore keeps the whole download in memory until it finishes**, and writes
  the scratch copy on an async worker. Fix: take the buffer by value and drop
  it once written, and write through `spawn_blocking`.
- **Backend error messages are English only.** Every `CommandError` carries an
  English message the frontend shows as is, the backup settings' refusals
  included, while the rest of the UI (and the native confirmation, §10) speaks
  12 languages. Fix: return a stable code with each error and let the frontend
  word it.
- **A compromised webview can still harm the data it can reach.** The native
  confirmation (§10) keeps backups from going to another server, but the
  webview's commands still edit and delete records, and back up and restore
  against the confirmed server without a native prompt: a script could empty
  the ledger and back that up over the only remote copy, which nothing
  versions. Fix: confirm a restore natively, and keep earlier backups on the
  server.

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
  A backup is held to 64 MiB both ways (`MAX_BACKUP_BYTES`): no larger download
  is read, so a hostile server can't exhaust memory, and no larger snapshot is
  uploaded, so no backup is made that restore would refuse. A ledger kept by
  hand stays far below it (250,000 entries with short notes come to about
  35 MB), and since a restored ledger is read whole for every month view, the
  cap also bounds the memory one can take.
- **Backup** is a `VACUUM INTO` snapshot — a clean single file with no
  `-wal`/`-shm` sidecars — read into memory and `PUT`. **Restore** must **not**
  swap the live `sqlx` pool (every command holds `State<SqlitePool>`; swapping
  would mean refactoring them all). Instead it replaces table contents **in
  place, in one transaction** with `PRAGMA defer_foreign_keys = ON`: `ATTACH` the
  downloaded file, then `DELETE` and `INSERT … SELECT` each data table in
  `TABLES` (plus `sqlite_sequence`, so AUTOINCREMENT counters don't collide),
  then `COMMIT`. Any failure rolls back and leaves local data intact; the
  connection that held the attachment is closed rather than returned to the
  pool.
- **A backup is validated before it replaces anything.** The downloaded file
  is untrusted. Attaching it is the one step that reads it unchecked: SQLite
  loads a file's statistics tables (`sqlite_stat1`/`sqlite_stat4`) as it
  attaches it, with no switch to skip that. So until the file is known to be
  ours it gets 60 s (`PRE_CHECK_BUDGET`, kept by a progress handler, which runs
  between SQLite's steps, so one long step can overrun it), and SQLite as a
  whole has a 256 MiB heap cap (`hard_heap_limit`, set at pool init): a file
  sized to stall restore or exhaust memory is refused instead of freezing it or
  getting the app killed. Restore then compares schema versions, first,
  so an older backup reads as another version's rather than a stranger's file;
  it reads the version only once `_sqlx_migrations` is a plain table with a
  stored `version` column. Every table it reads, `sqlite_sequence` included,
  must then have exactly this app's `CREATE` statement. Comparing the stored
  text evaluates nothing, whereas reading a view, a virtual table or a
  generated column runs SQL the file supplies. SQLite's `quick_check` must
  pass on each of those tables, scoped to them, since over the whole file it
  would evaluate the `CHECK` constraints of whatever else the file holds;
  nothing else in it is read. No table may hold more rows than restore allows
  (`TABLES`: 200 accounts, 500 categories, 1,000 rules, 2,000 budgets, 250,000
  entries, and their dependents to match), no rule or budget more amount
  breakpoints than its history may hold (99, the base amount aside), no text
  cell longer than the domain lets that column be (notes 1,000 characters;
  labels, names and icons 200; money, dates and codes 16; at four bytes a
  character), and `sqlite_sequence` no more than one in-range counter per
  table. No ledger kept by hand comes near those,
  while past them a crafted backup could exhaust memory as it is read back, or
  make every month view crawl, since the ledger walks each month from an
  account's anchor (§8) over all of its rows; a cell padded past any the app
  writes would even pass validation where the domain trims it. Every read of
  the file's tables skips its indexes (`NOT INDEXED`), since nothing checks
  them against their tables. The copy names each table's columns rather than
  selecting `*`: given `*`, SQLite copies the rows of a table without foreign
  keys as raw records, fields tacked on past the declared columns included.
  Rules and budgets are read one row each, their amount histories aggregated
  in the same statement, so a breakpoint never repeats its rule's note. After copying, still inside the transaction,
  restore reads everything back through the validating constructors, which
  also refuse membership rows under a normal account and an opening balance on
  a summary, and refuses ids above 2^52 or AUTOINCREMENT counters outside
  `0..=2^52` (ids cross to the frontend as JSON numbers, and one at `i64::MAX`
  would make every later insert fail; the loaders refuse a stored id that isn't
  positive, rather than alias it). It re-checks every summary account against
  the membership rules (§11) and that it owns no records, then checks foreign
  keys. Only then does it commit, so a refused backup leaves local data
  untouched. A failure is put down to the file only when it is the file's
  doing (damaged, not a database, a value that won't decode, a row the schema
  refuses, or out of time); anything else (a full disk, an I/O error) keeps its
  own error. Columns are read with `try_get`, never `get`, whose panic on an
  undecodable value would abort the app. The commands run backups and restores
  one at a time, each from end to end, network included, and their scratch
  copies are readable by the app's user alone.
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
  password. Saving with the password left blank keeps the stored one only
  while the address and username are unchanged, and drops it otherwise
  (clearing both fields disconnects): kept across a change, it would follow
  the address to whatever server answers there. OS-keychain storage is
  a tracked follow-up (the `keyring` crate doesn't cover Android, so it isn't a
  clean cross-platform win today).
- **A new server is confirmed natively.** Whatever saves the settings decides
  where the whole database is uploaded to and restored from, and the webview
  can save them, so a script running there that shouldn't (a compromised
  dependency, say) could point backups at a server of its own. Saving an
  address or username other than the stored ones, a first one included,
  therefore asks the user first, in a dialog the OS draws outside the webview,
  where no script can click it, reword it or cover it up (`tauri-plugin-dialog`,
  for the shell alone: the capabilities deny the webview its commands; see
  `confirm.rs` and `backup::save_settings`).
  - *What it shows.* The full address in the form requests use, international
    host names in their ASCII form so a look-alike can't pass for a familiar
    one, and the path too, since one host can serve several Nextclouds; then
    the username. Both must show whole and as they are, or the save is refused
    before anything is asked: an `https://` address of at most 128 characters,
    typed with nothing that doesn't print as itself, with no login, query or
    fragment, a host of letters, digits, `-` and `.` (or an IPv6 literal) in
    labels DNS can resolve, and a path of letters, digits and `-._~/` alone, so
    an `@` or `:` can't dress the path up as a familiar host; and a username of
    at most 64 of the characters Nextcloud allows in a user id (ASCII letters,
    digits, spaces and `_.@-'`), so no line break, direction mark, invisible
    character or letter from another script can disguise it. Neither can hold a
    `%`, which the Linux dialog (`rfd`'s GTK backend) reads as a `printf`
    format.
  - *Its wording* is the shell's own: `src-tauri/locales/native-dialogs.json`,
    compiled in by `build.rs`, so the webview can't change it at runtime, and
    the OS's language, never the webview's, picks which. (On Android only the
    first system language is known, so a user whose first language the app
    doesn't speak sees it in English even where the app's own text is in their
    second.) It words the consent itself, so an edit to it deserves the review
    shell code gets; a frontend test holds its languages to the app's, and a
    wording whose two buttons the plugin couldn't tell apart is refused rather
    than shown.
  - *What counts as yes.* Only the confirm button, and only a second and a half
    or more after the dialog was asked for: a script can't press it, but it
    could open the dialog just as the user taps where the button will appear. A
    confirm sooner than that is refused, says why, and counts as a decline, so
    luring taps gets no more tries than being turned down. Even then the save
    goes through only if the stored server is still the one the question was
    decided against, and reads and writes of `nextcloud.json` take one lock, so
    no save slips between another's read and write. Clearing both fields
    (disconnecting) needs no confirmation, nor does a new password for the
    stored server: a script can still stop backups, just not redirect them.
  - *How often it asks.* One question is asked at a time, and a dialog holds the
    screen until it is answered: one left unanswered for two minutes counts as
    no but keeps the screen, until it has been up for ten minutes, past which it
    is taken to be gone (the OS can take one away without answering) and another
    may open. After three declines in a row the app stops asking for ten
    minutes, each further decline doubling that up to a day, until a server is
    confirmed. Both run by the wall clock, since a phone's monotonic clock stops
    while it sleeps, and a clock set back further than either lasts ends it
    rather than stretching it. A dialog that couldn't be shown is an error, not
    a decline, except on mobile, where the plugin reports one as a press of
    Cancel.
  - *Accepted limits.* A server saved before this check existed is kept as it
    is, unconfirmed and unchecked, until it is changed. A dialog past its two
    minutes stays on screen, since the plugin can't close it, but confirming it
    saves nothing. A script that lets dialogs lapse holds off setting up a
    server for as long as the pause lasts, or until the app restarts. The read
    time is counted from the request to show the dialog, not from when it
    appears, and a script can keep the main thread busy (Tauri's own sync
    commands, the IPC requests themselves), so the dialog may appear with some
    of it spent: a partial defence. The Settings screen's side of a declined
    save (reporting it, keeping the typed password) has no automated test, as
    the frontend has no DOM test harness. And the dialog guards the server, not
    the user's reading of it: a script could swap its own server in for the one
    the user just typed, and only the dialog's address line would tell.
  - *The last backup* counts only for the server it was uploaded to, so moving
    the settings mid-upload doesn't credit the new server with it.
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
  it. No nesting: members must be normal accounts. These cross-account rules
  (plus: at least one member, each existing, none the summary itself) live in
  `core` as `Account::check_members`, which takes a way to look the members up;
  the constructors can't see other accounts, so they check only structural
  rules: a normal account has no members, a summary has a zero balance and
  distinct members. The command layer applies it whenever a summary is saved,
  and restore to every summary in a backup (§10). Each account command runs its
  checks (a summary's members, whether an account belongs to one) and its write
  in one transaction that holds the write lock from its start
  (`db::begin_write`, `BEGIN IMMEDIATE`), so no other write can land between
  them: two account changes made at once can't leave a summary with no members,
  or over accounts of mixed currencies. A transfer, which needs both its
  accounts in one currency, does the same. So do entries, rules, budgets and
  single-occurrence edits, checking that their account isn't a summary
  (`reject_if_summary`): a restore committed in between could otherwise bring
  the id back as a summary, leaving it owning a record. A second budget for a
  category is refused by the database too.
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

---

## 12. Category budgets — 🟢 DONE (implemented)

Everyday use exposed a gap in §1. A planned monthly cost that leaves the account
in one lump (rent, a subscription) is a **recurring rule**. But a planned cost
spent as many purchases across the month — groceries being the obvious one — has
no good shape here: model it as a recurring rule and the rule's expansion *and*
the real purchases both hit the ledger, so `available` is understated by the
whole planned amount all month.

A **budget** is the missing shape: a planned monthly amount for one category that
is **held back**, and that the month's real spending in that category
**consumes** rather than adds to.

```
reserved = Σ over budgets of max(0, planned − actual)
free     = available − reserved
```

- **`available` is untouched.** It keeps its §1 meaning exactly
  (`carry_in + income − expenses`), so `balance_at_end_of` and the carry-over
  chain are budget-independent — `summaries_for_range` still chains `available`,
  never `free`. A budget moves no money; it only annotates a month.
- **Still not envelope budgeting.** There is no funding, no rollover, and no
  limit enforced at entry time. An unspent reservation simply **expires** at
  month end and the money carries into the next month as ordinary surplus. This
  amends §1's "not per-category limits" only in that categories may now carry a
  *plan*; they are still descriptive, and nothing is ever refused for exceeding
  one.
- **One category each.** At most one budget per category per account
  (`UNIQUE (account_id, category_id)` plus a friendlier command-layer check).
  `core` can't see that cross-row rule, exactly as with summary membership (§11).
- **`actual` is what Statistics reports.** `core::budget_status` derives it from
  `core::expenses_by_category`, so ad-hoc entries, recurring expansions, and
  per-occurrence skips are all handled once, identically to the stats screen, in
  one pass shared by every budget.
- **Recurring-rule collision.** A recurring *expense* in a budgeted category
  books its full amount every month, so it consumes the whole budget on day one
  and leaves it inert while still double-counting against `available`. The
  combination is legal (a fixed rule plus a budget for extras is reasonable), so
  neither `core` nor the command layer refuses it; instead the budget form
  detects it, seeds the amount from the rule, and offers to delete it, and the
  Budgets screen flags any row where one still exists.
- **UI.** Its own nav tab beside Recurring, month-aware like Statistics, with a
  planned/actual progress bar per budget. On the month list, a row measured by a
  budget carries a 🎯 badge next to the 🔁 one a recurring occurrence gets — the
  same "here is why this row is more than a number" cue. Expenses only, since a
  budget measures spending and `expenses_by_category` ignores income. The month bar gains a fourth figure,
  **Free**, shown only when the month actually has budgets; the budget ring and
  the home-screen widget now follow `free` rather than `available` — "can I spend
  this?" is the question the ring is really answering. With no budgets,
  `reserved` is zero and every figure reduces to the pre-budget behaviour.
- **Summary accounts.** `reserved` is summed **per member**, not re-derived from
  merged totals: one member overspending a category must not cancel another
  member's reservation in it. The budget list concatenates members' rows (tagged
  by source account, as entries are), which keeps `Σ status.reserved` equal to
  the headline `reserved`. Summaries own no budgets and are read-only here.
- **Persistence.** Migration `0005` adds the `budget` table and `0006` its
  `budget_amount` history (money TEXT, months as `(year, month)` INTEGER pairs
  like `account.anchor_*`). `category_id` cascades — unlike an entry, a budget
  names exactly one category and has nothing left to measure without it. Both
  tables join the backup/restore `TABLES` set. Note `0006`'s header comment still
  says the start month is read-only, which it no longer is: `sqlx` checksums
  applied migrations, so editing even a comment would make every existing install
  refuse to start. Applied migrations are immutable — this file is the record.
- **Effective-dated amounts.** A budget carries an **amount history**, not one
  figure: `budget.amount` is the base at its start month, and migration `0006`'s
  `budget_amount` table holds later breakpoints — the same shape and rationale as
  `0002_rule_amount.sql` for rules, with `Budget::amount_in(month)` as the single
  read point. Raising groceries from 400 to 450 in June leaves January–May
  reserving 400, so past months keep the figure that was actually planned at the
  time. The form offers the same "From {{month}} onward / All months" choice
  `RuleForm` does, defaulting to *onward*; it is offered only once the budget has
  a past to protect. The stakes are lower than for a rule — a rule's amount feeds
  the ledger, so a retroactive edit would move historical *balances*, whereas a
  budget's only feeds `reserved`, which never enters the chain — but showing a
  month a plan it never had is still wrong.
- **How far a change reaches is an explicit three-way choice**, because "I only
  meant to change this one month" is the mistake this form invites. Editing an
  amount offers **{{month}} only** (the default — the narrowest), **From
  {{month}}**, and **All months**, each spelling out its own effect beneath the
  control:
  - *only* writes the month's segment **and** one after it restoring whatever
    was running there. A single-month change is just two breakpoints, so the
    segment model expresses it exactly. Without it the only ways to lower one
    month were to lower every later month too, or to end the budget after it —
    which throws the later months away entirely.
  - *from* writes the month's segment and leaves every other breakpoint alone,
    so the new figure runs until the next planned change. Note this differs from
    the recurring-rule form, which drops later breakpoints: a rule's effective
    date is hand-picked and almost always the newest point, whereas a budget's is
    whichever month the screen happens to be showing, and paging back to correct
    an earlier month must not wipe the plan for later ones.
  - *all* flattens the history to one segment. It is the only way to clear a
    history, including back under `MAX_AMOUNT_SEGMENTS`.
  The control is labelled with the month the write actually lands on, which is
  the viewed month clamped into the budget's span — offering "From July" and then
  writing August was a real bug.
- **The start month is editable**, and re-anchors the history
  (`lib/budget.ts::reanchorAmounts`). Moving it **earlier** extends the base back
  over months that had no budget at all, so it cannot falsify anything — there
  was nothing there. Moving it **later** drops the segments outside the new span
  and re-anchors on whatever amount was in effect at the new start. This is the
  only way to bring an existing budget back to a month you paged to, and since a
  category holds at most one budget it is the only way to cover that month at
  all; the create form says as much when it hides an already-budgeted category.
  Opening a budget from a month it doesn't reach **proposes that month** in the
  start field (and symmetrically for the end, past a finished budget) — that is
  why you opened it, and a field showing the proposal beats a notice explaining
  that you have to go and change one. The cost is that saving an untouched form
  extends the budget; it is visible in the field and non-destructive, since it
  only covers months that had nothing.
- **Deferred.** `UNIQUE (account_id, category_id)` is unconditional over time, so
  a category has at most one budget row ever. Dated amounts remove the reason
  that used to matter (changing the plan going forward no longer needs a
  successor row); what remains unexpressible is a *gap* — budgeted Jan–Jun,
  unbudgeted Jul–Sep, budgeted again from Oct. Supporting that means relaxing the
  constraint to "no two **overlapping** budgets per category", which SQLite can't
  state declaratively: it needs a table rebuild plus a command-layer overlap
  check. Also deferred: envelope rollover, budgets spanning several categories,
  non-monthly periods.

---

## 13. Balance today — 🟢 DONE (implemented)

Everyday use exposed drift: forget to record one purchase and the ledger quietly
parts ways with the real account, with nothing to compare it against — every
figure in the month bar is a month-end projection. **Balance today** is that
comparison point: what the account should hold *right now* if every transaction
was recorded. When it disagrees with the bank, something is missing (or extra),
and the size of the gap says what to look for.

```
balance_as_of(date) = opening_balance + Σ entries and occurrences dated ≤ date
```

- **The §1 prefix sum at day granularity.** `core::balance_as_of` is
  `balance_at_end_of` for the previous month plus the current month's activity up
  to and **including** `date`; on a month's last day the two agree exactly
  (unit-tested). An entry dated today counts; one pre-recorded for later this
  month doesn't yet — which is exactly how today's figure can differ from
  `available`. A recurring occurrence counts from its scheduled day, a skipped
  one never. A date before the anchor month yields the opening balance, as in §1.
  Budgets play no part: like `available`, it is the real balance, never `free`.
- **Current month only.** The bar shows it above Income only while viewing the
  month that contains today; a point-in-time figure beside another month's
  totals would read as belonging to that month.
- **Its own row, its own state.** The figure has its own query, and while it
  loads or if it fails only its row says so (`…` / `—`): the month's figures
  never wait for it or vanish with it. Unlike `free`, nothing else in the bar is
  derived from it, so a split state can't leave two figures contradicting.
- **"Today" comes from the device clock, via the frontend.** `core` has no clock
  by design, and the shell can't reliably read the local offset (`time`'s
  `now_local` refuses once the process is multithreaded), so the frontend passes
  its local date to the `balance_as_of` command. `useToday` re-reads it at local
  midnight and whenever the app returns to the foreground (a backgrounded
  webview suspends its timers); the date is part of the query key, so a new day
  fetches afresh, and once the month turns the row leaves the old month's bar.
- **Summary accounts** sum their members' balances, each against its own opening
  balance and anchor — the same per-member rule as §11.
- **Display only.** Nothing is stored and no money moves. It stays in-app: the
  widget boundary (§6) is unchanged, so no absolute figure reaches the OS.
- **Deferred.** A *reconcile* action — enter the bank's figure, book the
  difference as a correction entry dated today — would close the loop on fixing
  drift; for now that entry is added by hand.
