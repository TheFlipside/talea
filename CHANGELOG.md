# Changelog

All notable changes to this project are documented in this file.

## Unreleased

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
  opposite kind ("also record as income/expense on …"). A new atomic
  `create_transfer` command writes both entries in one transaction (no currency
  conversion — only same-currency accounts are offered). The two entries are
  independent afterward. Adds `EntryKind::opposite`.
- A **Home** button is now the leftmost icon in the navigation bar, switching
  back to the month view.

### Changed

- The date picker's calendar now renders in normal flow rather than as an
  absolute popup, so the entry/rule modal grows to use the available viewport
  height instead of clipping the calendar and forcing a scroll.

### Fixed

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

