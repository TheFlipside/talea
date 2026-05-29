# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

- Initial project scaffold: Cargo workspace with `core/`, `src-tauri/`, and
  `frontend/`, plus `README.md` and architecture/decisions in `CLAUDE.md`.
- `core` crate: `Money` newtype over `rust_decimal::Decimal` (no floating
  point), with construction/formatting helpers and unit tests.
- Stubbed domain model in `core` with the budgeting-model design decision
  deliberately deferred and tracked in `docs/DESIGN.md`.
- Minimal Tauri 2 + React/TypeScript (Vite) smoke screen bridging `core` to the
  frontend via a typed `invoke` command.
- Quality gates pass clean from the first commit: `cargo clippy -W
  clippy::pedantic -D warnings`, `cargo fmt --check`, and
  `eslint --max-warnings=0`.

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

