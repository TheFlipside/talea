# Talea

A **local-first, cross-platform budget app**. Your money data lives on your
device in a local SQLite database — no account, no cloud, no sync server in the
loop. Talea targets **Android and iOS** first, with the desktop build used for
day-to-day development.

> **Status:** Early development. The **`core` domain model** (a monthly cashflow
> ledger with carry-over), the **SQLite persistence layer + typed Tauri command
> surface**, and the **Phase-1 UI core loop** (accounts, the month screen with a
> budget ring, entry CRUD, month navigation) are implemented and tested. Next:
> category / recurring-rule management and the stats screen — see
> [The budgeting model](#the-budgeting-model).

## Why "local-first"

- The database is the source of truth and it sits on the device.
- The app is fully functional offline.
- Sensitive figures stay on-device behind a biometric lock (planned milestone).

## Architecture

Talea is a **Cargo workspace** with a strict separation between pure domain
logic and the platform shell:

```
talea/
├── core/        → pure-Rust domain + money math. No Tauri, no IO, no SQL.
│                  Fully unit-tested. The "what the app means" layer.
├── src-tauri/   → the Tauri shell. Bridges `core` to the frontend via
│                  commands, owns persistence (SQLite via sqlx) and platform
│                  integration. The "how it runs on a device" layer.
└── frontend/    → React + TypeScript (Vite). The UI. Talks to the shell
                   only through Tauri's typed `invoke` boundary.
```

The dependency direction is one-way and enforced by crate boundaries:

```
frontend ──invoke──▶ src-tauri ──calls──▶ core
                         │
                         └── sqlx ──▶ SQLite (on device)
```

`core` knows nothing about Tauri, the filesystem, or SQL. That keeps the
budgeting rules and money arithmetic testable in isolation and portable should
the shell ever change.

### Key technical decisions

| Decision        | Choice                          | Rationale                                                                 |
| --------------- | ------------------------------- | ------------------------------------------------------------------------- |
| Shell           | Tauri 2.x                       | One Rust core, native Android + iOS + desktop, small footprint.           |
| Frontend        | React + TypeScript (Vite)       | Familiar, fast HMR, typed boundary to the shell.                          |
| Persistence     | local SQLite via `sqlx`         | Local-first, embeddable, queryable, no server.                            |
| Money           | `rust_decimal` — **never f64**  | Exact base-10 arithmetic; floating point is forbidden for monetary values.|
| Domain location | pure `core` crate               | Logic stays IO-free and unit-tested, isolated from the shell.             |

## The budgeting model

Talea is a **monthly cashflow ledger with carry-over** — not envelope budgeting
and not per-category limits:

- Each month's *available to end of month* = `carry_in + income − expenses`
  (ad-hoc entries **plus** expanded recurring rules). A month's ending balance
  **carries into** the next, per account.
- **Accounts** each hold a fixed currency and an opening balance; **categories**
  are a global, descriptive list (label + icon/emoji); **entries** are signed by
  kind (income/expense); **recurring rules** expand into per-month occurrences
  (weekly/monthly/yearly, every-N, with month-end/leap-day clamping).

This is implemented and unit-tested in `core`. Full rationale and the remaining
details live in [`docs/DESIGN.md`](docs/DESIGN.md).

**Still open:** the per-screen UI (the domain is persisted and reachable through
typed Tauri commands).

## Prerequisites

- Rust (stable, ≥ 1.80) with `cargo`.
- Node.js ≥ 20 and npm.
- Tauri 2 system dependencies for your platform — see the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

## Getting started

```bash
# Install frontend dependencies
npm --prefix frontend install

# Run the desktop dev build (starts Vite, then the Tauri shell)
cargo tauri dev

# Production build
cargo tauri build
```

## Quality gates

All of these must pass clean from the first commit:

```bash
cargo clippy --workspace --all-targets -- -W clippy::pedantic -D warnings
cargo fmt --all --check
cargo test --workspace
npm --prefix frontend run lint     # eslint --max-warnings=0
npm --prefix frontend run build    # tsc + vite build
```

SQL in `src-tauri` is compile-time checked by `sqlx::query!` against a committed
`.sqlx/` offline cache (`SQLX_OFFLINE=true` in `.cargo/config.toml`), so the
gates build with **no database**. After changing any query, regenerate the cache
and commit it:

```bash
# one-time: a matching sqlx-cli
cargo install sqlx-cli --version ^0.9 --no-default-features --features sqlite
# regenerate .sqlx against a scratch DB migrated from src-tauri/migrations
export DATABASE_URL="sqlite:///tmp/talea-prepare.sqlite3"
sqlx database create && sqlx migrate run --source src-tauri/migrations
cargo sqlx prepare --workspace          # then commit the updated .sqlx/
```

### Resetting local data

The app stores its SQLite database in the OS app-data directory under the
identifier `app.talea.budget`. Deleting it gives a clean first run (which
auto-creates a default account):

```bash
# Linux
rm -f ~/.local/share/app.talea.budget/talea.sqlite3*
# macOS
rm -f ~/Library/Application\ Support/app.talea.budget/talea.sqlite3*
# Windows (PowerShell)
Remove-Item "$env:APPDATA\app.talea.budget\talea.sqlite3*"
```

(The `talea.sqlite3*` glob also removes the `-wal`/`-shm` WAL sidecar files.)

## Roadmap (selected)

- [x] Decide the budgeting model (monthly cashflow ledger with carry-over).
- [x] Core domain logic + full unit tests (money, entries, recurrence, ledger).
- [x] SQLite schema + persistence layer in `src-tauri` (sqlx + migrations).
- [x] Typed Tauri commands exposing the domain to the frontend.
- [x] Phase-1 UI: account onboarding/switch, month screen (income/expenses/
      available + budget ring), entry CRUD, prev/next + swipe month navigation.
- [x] Phase-2 foundation: internationalization (react-i18next), a settings
      screen (theme, language, budget-ring meaning), an icon navigation bar, and
      a Manage Accounts screen (add/edit/delete).
- [x] Category management (emoji picker, common defaults) wired into entries.
- [x] Statistics screen: per-month expenses broken down by category (with
      uncategorized expenses folded into an "Other" slice).
- [x] Recurring-rule management: add/edit/delete rules with effective-dated
      amounts (a change applies forward without rewriting the past); occurrences
      show in the month list, where a single one can be removed (skipped) or
      edited (detached into a standalone entry).
- [x] Optional biometric app lock (mobile; a Settings toggle gates the app on
      launch via `tauri-plugin-biometric`, with graceful degradation where
      biometrics are unavailable).
- [ ] **Home-screen widget:** an abstract ring / color indicator only — the
      actual figures stay in-app behind the biometric lock. *(Later milestone.)*

## License

MIT.
