# Project: Talea

## What This Project Does

Talea is a **local-first, cross-platform budget app**. All financial data lives
on-device in a local SQLite database — no cloud, no account, no sync server.
Primary targets are **Android and iOS**; the desktop build is for development.

## Stack

- **Language:** Rust (workspace) + TypeScript (frontend).
- **Shell:** Tauri 2.x (Android + iOS primary, desktop for dev).
- **Frontend:** React + TypeScript, built with Vite.
- **Build:** Cargo (workspace) + npm/Vite. `cargo tauri dev` / `cargo tauri build`.
- **Test:** `cargo test` (core domain logic); frontend tests TBD.
- **Key deps:** `tauri`, `rust_decimal` (money), `sqlx` (SQLite), `serde`, React.

## Directory Layout

```
core/        → pure-Rust domain + budgeting logic + money math.
               NO Tauri, NO IO, NO SQL. Fully unit-tested in isolation.
src-tauri/   → the Tauri shell. Bridges core to the frontend (commands),
               owns persistence (SQLite via sqlx) and platform integration.
frontend/    → React + TypeScript (Vite). UI only; talks to the shell
               solely through Tauri's typed `invoke` boundary.
docs/        → design docs. See DESIGN.md for open decisions.
```

Dependency direction is one-way: `frontend → src-tauri → core`. `core` must
never depend on Tauri, the filesystem, or SQL.

## Architecture & Decisions (binding)

- **Clean separation of concerns.** Domain logic and money math live in the
  pure `core` crate, kept entirely free of Tauri/IO/SQL so it stays portable
  and unit-testable. `src-tauri` is a thin shell that bridges `core` to the
  frontend and owns all IO. Mirror this split in all future work.
- **Money is never floating point.** Every monetary value uses
  `rust_decimal::Decimal`. `f32`/`f64` for money is forbidden, including in
  serialization and at the frontend boundary (money crosses as strings, not
  numbers). Reject any change introducing float money.
- **Persistence is local SQLite via `sqlx`**, owned exclusively by `src-tauri`.
  `core` receives data through plain types, never a connection.

## Open Design Decisions — DO NOT skip

Deliberately unresolved; **must be decided before the SQLite schema is written**.
Do not finalize the schema or invent these on the fly.

- **Budgeting model:** envelope vs. flexible vs. hybrid. Drives the
  `month` / `category` / `budget` / `transaction` relationships.
- **SQLite schema:** blocked on the above.

The `core` domain model is currently **stubbed** with these questions marked
inline (`DESIGN DECISION:` / `TODO`) and tracked in `docs/DESIGN.md`. When asked
to work on the model or schema, surface these decisions first.

## Later Milestones (not in this scaffold)

- **Home-screen widget:** shows only an **abstract ring / color** indicator of
  budget health. The actual figures never appear on the widget — they stay
  **in-app behind a biometric lock**. Deliberate later milestone, intentionally
  NOT part of the initial scaffold.

## Essential Commands

```bash
npm --prefix frontend install     # one-time: install frontend deps
cargo tauri dev                   # desktop dev build (Vite + shell)
cargo tauri build                 # production build
cargo test --workspace            # run core unit tests
```

## Project-Specific Rules

- **Pre-commit gate (binding).** Before every commit, run `/review` and
  `/security-audit` and address all findings — fix them, or get explicit user
  sign-off to defer with a tracked follow-up. No commit ships with unresolved
  findings from either skill. Applies to every commit, including small ones.
- Document changes in `CHANGELOG.md`, following its category convention.
- **All quality gates pass clean, from the first commit:**

  ```bash
  cargo clippy --workspace --all-targets -- -W clippy::pedantic -D warnings
  cargo fmt --all --check
  npm --prefix frontend run lint     # eslint --max-warnings=0
  ```

- **Final fmt pass before staging (binding).** The very last action before
  `git add` for any commit is `cargo fmt --all && cargo fmt --all -- --check`.
  The `--check` asserts the working tree matches what CI's
  `cargo fmt --all -- --check` will run on. An fmt run earlier in the gate is
  **not** sufficient — every subsequent edit invalidates that snapshot
  (rustfmt 1.95+ is layout-aware and may re-collapse multi-line calls).

## Skills Available

- `codebase-navigator` — use when first exploring this repo.
- `code-quality` — use before committing any changes.

## See Also

@README.md
@docs/DESIGN.md
