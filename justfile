# Talea task runner. Run `just` (or `just --list`) to see all recipes.
# Requires: cargo, cargo-tauri, node/npm. The `crap` recipe also needs
# `cargo install cargo-llvm-cov cargo-crap` and the llvm-tools component.

# JDK 17 + Android SDK/NDK for the mobile recipes. JAVA_HOME defaults to the apt
# OpenJDK 17 path and ANDROID_HOME to the Android Studio default; override either
# via the environment. The NDK path is auto-detected per recipe. See
# docs/DEVELOPMENT.md.
export JAVA_HOME := env_var_or_default("JAVA_HOME", "/usr/lib/jvm/java-17-openjdk-amd64")
export ANDROID_HOME := env_var_or_default("ANDROID_HOME", env_var("HOME") / "Android" / "Sdk")

# Show the available recipes.
default:
    @just --list

# Install frontend dependencies (one-time).
install:
    npm --prefix frontend install

# Install frontend deps only if missing, so a fresh clone's first `just dev`
# (or build/test) doesn't fail on the not-yet-installed Vite toolchain.
_ensure-frontend:
    #!/usr/bin/env bash
    set -euo pipefail
    [ -d frontend/node_modules ] || npm --prefix frontend install

# Run the desktop dev build (starts Vite, then the Tauri shell).
dev: _ensure-frontend
    cargo tauri dev

# Production build.
build: _ensure-frontend
    cargo tauri build

# Format all Rust code.
fmt:
    cargo fmt --all

# Run all tests (Rust workspace + frontend unit tests).
test: _ensure-frontend
    cargo test --workspace
    npm --prefix frontend run test

# Lint everything without mutating (Rust + frontend).
lint: _ensure-frontend
    cargo clippy --workspace --all-targets -- -W clippy::pedantic -D warnings
    cargo fmt --all -- --check
    npm --prefix frontend run typecheck
    npm --prefix frontend run lint

# Full pre-commit gate: lint + tests + builds, exactly as CI expects.
gate: _ensure-frontend
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
    rm -f ~/.local/share/com.luminaapps.talea/talea.sqlite3*
    @echo "Local dev database reset."

# CRAP diagnosis: generate Rust coverage, then score change-risk/complexity.
crap:
    cargo llvm-cov --lcov --output-path lcov.info
    cargo crap --lcov lcov.info

# ---- Android (mobile) ----
# Requires the prerequisites in docs/DEVELOPMENT.md. NDK_HOME is taken from the
# environment or auto-detected as the latest NDK under $ANDROID_HOME/ndk.

# Generate the native Android project (one-time; output is gitignored).
# `android init` scaffolds the project with the default Tauri launcher icon, so
# reapply the branded icons from the manifest right after.
android-init:
    #!/usr/bin/env bash
    set -euo pipefail
    export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
    cargo tauri android init
    cargo tauri icon src-tauri/icons/icon-manifest.json

# Run on a connected device over USB (maps the device's localhost via adb reverse).
android-dev: _ensure-frontend
    #!/usr/bin/env bash
    set -euo pipefail
    export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
    cargo tauri android dev

# Run on a device over the LAN (most reliable for physical devices). Pass your
# machine's LAN IP, e.g. `just android-dev-host 192.168.1.20`.
android-dev-host ip: _ensure-frontend
    #!/usr/bin/env bash
    set -euo pipefail
    export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
    export TAURI_DEV_HOST="{{ip}}"
    cargo tauri android dev --host "{{ip}}"

# Build a release APK/AAB (output under src-tauri/gen/android).
android-build: _ensure-frontend
    #!/usr/bin/env bash
    set -euo pipefail
    export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
    cargo tauri android build

# Tail device logs for the running app (webview console + Rust stdout/stderr).
android-log:
    #!/usr/bin/env bash
    set -euo pipefail
    pid="$(adb shell pidof -s com.luminaapps.talea || true)"
    if [ -z "$pid" ]; then echo "Talea isn't running on the device."; exit 1; fi
    adb logcat --pid="$pid"

# Wipe the app's on-device data (database + lock preference) for a clean run.
android-reset:
    adb shell pm clear com.luminaapps.talea

# ---- iOS (mobile; macOS + Xcode only) ----
# Requires Xcode and an Apple development team. Set the team ONCE, either:
#   • tauri.conf.json → bundle.iOS.developmentTeam = "XXXXXXXXXX", or
#   • export APPLE_DEVELOPMENT_TEAM=XXXXXXXXXX  (inherited by these recipes).
# Drive every build through the tauri-cli — never Xcode's Run button (that
# bypasses the cli's WebSocket and fails). See docs/DEVELOPMENT.md.

# `ios init` scaffolds default icons, so reapply the branded ones right after.
# Generate the native iOS project (one-time) and apply the branded icons.
ios-init:
    cargo tauri ios init
    cargo tauri icon src-tauri/icons/icon-manifest.json

# Run live on a connected device / simulator (Vite dev server + HMR).
ios-dev: _ensure-frontend
    cargo tauri ios dev

# Build a signed App Store IPA → src-tauri/gen/apple/build/ (upload via Transporter).
ios-release: _ensure-frontend
    cargo tauri ios build --export-method app-store-connect
