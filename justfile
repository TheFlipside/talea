# Talea task runner. Run `just` (or `just --list`) to see all recipes.
# Requires: cargo, cargo-tauri, node/npm. The `crap` recipe also needs
# `cargo install cargo-llvm-cov cargo-crap` and the llvm-tools component.

# Show the available recipes.
default:
    @just --list

# Install frontend dependencies (one-time).
install:
    npm --prefix frontend install

# Run the desktop dev build (starts Vite, then the Tauri shell).
dev:
    cargo tauri dev

# Production build.
build:
    cargo tauri build

# Format all Rust code.
fmt:
    cargo fmt --all

# Run all tests (Rust workspace + frontend unit tests).
test:
    cargo test --workspace
    npm --prefix frontend run test

# Lint everything without mutating (Rust + frontend).
lint:
    cargo clippy --workspace --all-targets -- -W clippy::pedantic -D warnings
    cargo fmt --all -- --check
    npm --prefix frontend run typecheck
    npm --prefix frontend run lint

# Full pre-commit gate: lint + tests + builds, exactly as CI expects.
gate:
    cargo clippy --workspace --all-targets -- -W clippy::pedantic -D warnings
    cargo fmt --all -- --check
    cargo test --workspace
    npm --prefix frontend run typecheck
    npm --prefix frontend run lint
    npm --prefix frontend run test
    npm --prefix frontend run build

# Regenerate the committed sqlx offline cache after changing any query!.
sqlx-prepare:
    #!/usr/bin/env bash
    set -euo pipefail
    export DATABASE_URL="sqlite://$(mktemp -d)/talea-prepare.sqlite3"
    sqlx database create
    sqlx migrate run --source src-tauri/migrations
    cargo sqlx prepare --workspace
    echo "Updated .sqlx/ — commit it."

# Delete the local dev database for a clean first run (Linux; see README for other OSes).
reset-db:
    rm -f ~/.local/share/app.talea.budget/talea.sqlite3*
    @echo "Local dev database reset."

# CRAP diagnosis: generate Rust coverage, then score change-risk/complexity.
crap:
    cargo llvm-cov --lcov --output-path lcov.info
    cargo crap --lcov lcov.info
