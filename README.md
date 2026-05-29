# Talea

A **local-first, cross-platform budget app**. Your money data lives on your
device in a local SQLite database — no account, no cloud, no sync server in the
loop. Talea targets **Android and iOS** first, with the desktop build used for
day-to-day development.

> **Status:** Scaffold. The app builds and runs a minimal smoke screen. The
> budgeting domain model and the SQLite schema are intentionally *not* finalized
> yet — see [Deliberate open decisions](#deliberate-open-decisions).

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

## Deliberate open decisions

These are **design choices to be made before** the schema is written, not
oversights:

- **Budgeting model:** envelope vs. flexible (or a hybrid). This shapes the
  `month` / `category` / `budget` / `transaction` relationships and therefore
  the entire schema.
- **SQLite schema:** not finalized until the above is decided.

The domain model in `core` is currently **stubbed** with these questions called
out inline and in [`docs/DESIGN.md`](docs/DESIGN.md).

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

## Roadmap (selected)

- [ ] Decide the budgeting model and finalize the SQLite schema.
- [ ] Persistence layer in `src-tauri` (sqlx + migrations).
- [ ] Core budgeting logic + full unit tests.
- [ ] **Home-screen widget:** an abstract ring / color indicator only — the
      actual figures stay in-app behind a biometric lock. *(Later milestone,
      not part of this scaffold.)*

## License

MIT.
