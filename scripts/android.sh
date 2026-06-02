#!/usr/bin/env bash
#
# Talea Android build & sign helper.
#
# Subcommands:
#   apk       Build + sign a universal release APK for a test device, then print
#             the `adb install` command.
#   aab       Build + sign the Play Store bundle; copy "<App>-<ver>.aab" and the
#             R8 "<App>-<ver>-mapping.txt" to the Desktop.
#   symbols   Build with native symbols retained and zip them to
#             "<App>-<ver>-native-symbols.zip" on the Desktop (optional Play
#             upload for symbolicating native/Rust crashes).
#   release   aab + symbols — the full Play Store upload set.
#
# The signing keystore/alias come from $TALEA_KEYSTORE / $TALEA_KEY_ALIAS
# (defaults below). The password is prompted once up front and handed to the
# signers via the environment (never on the command line / in the process list),
# so the rest runs unattended.
#
# JAVA_HOME is forced to a JDK <= 21 (Gradle/AGP and the generated buildSrc's
# Kotlin compiler reject newer majors, e.g. JDK 25), NDK is auto-detected as the
# latest under $ANDROID_HOME/ndk, and build-tools as the latest installed.
# See docs/DEVELOPMENT.md.
set -euo pipefail

# ---- configuration (override via environment) -------------------------------
KEYSTORE="${TALEA_KEYSTORE:-$HOME/play-store_release-key.keystore}"
KEY_ALIAS="${TALEA_KEY_ALIAS:-play-store_release}"
DESKTOP="${TALEA_DESKTOP:-$HOME/Desktop}"

# ---- repo root (this script lives in scripts/) ------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

ANDROID_DIR="src-tauri/gen/android"
APK_DIR="$ANDROID_DIR/app/build/outputs/apk/universal/release"
AAB="$ANDROID_DIR/app/build/outputs/bundle/universalRelease/app-universal-release.aab"
MAPPING="$ANDROID_DIR/app/build/outputs/mapping/universalRelease/mapping.txt"
JNILIBS="$ANDROID_DIR/app/src/main/jniLibs"

# ---- helpers ----------------------------------------------------------------
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }

# Major version of the JDK at $1 (e.g. "17"), or empty if it can't be read.
java_major() { "$1/bin/java" -version 2>&1 | sed -n 's/.*version "\([0-9]*\).*/\1/p' | head -1; }

app_name() { jq -r '.productName' src-tauri/tauri.conf.json; }
app_version() { jq -r '.version' src-tauri/tauri.conf.json; }

# Latest versioned subdirectory of $1 (NDK / build-tools), or empty.
latest_dir() { find "$1" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort -V | tail -1; }

setup_env() {
    export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
    [ -d "$ANDROID_HOME" ] || die "ANDROID_HOME not found: $ANDROID_HOME"

    if [ -z "${NDK_HOME:-}" ]; then
        NDK_HOME="$(latest_dir "$ANDROID_HOME/ndk")"
    fi
    [ -d "$NDK_HOME" ] || die "no NDK under $ANDROID_HOME/ndk (set NDK_HOME)"
    export NDK_HOME

    # Pick the first JDK <= 21 from JAVA_HOME (if usable), then the usual paths.
    local candidates=() chosen="" cand maj
    [ -n "${JAVA_HOME:-}" ] && candidates+=("$JAVA_HOME")
    candidates+=(
        "/usr/lib/jvm/java-17-openjdk-amd64"
        "/usr/lib/jvm/java-21-openjdk-amd64"
        "$HOME/Dev/android-studio/jbr"
    )
    for cand in "${candidates[@]}"; do
        [ -x "$cand/bin/java" ] || continue
        maj="$(java_major "$cand")"
        if [ -n "$maj" ] && [ "$maj" -le 21 ]; then chosen="$cand"; break; fi
    done
    [ -n "$chosen" ] || die "need a JDK 17 or 21 — set JAVA_HOME (the default 'java' may be too new for Gradle)"
    export JAVA_HOME="$chosen"

    info "JAVA_HOME=$JAVA_HOME (JDK $(java_major "$JAVA_HOME"))"
    info "NDK_HOME=$NDK_HOME"
}

ensure_frontend() { [ -d frontend/node_modules ] || npm --prefix frontend install; }

# Prompt once for the keystore password into the environment (TALEA_KS_PASS).
prompt_password() {
    [ -f "$KEYSTORE" ] || die "keystore not found: $KEYSTORE (set TALEA_KEYSTORE)"
    if [ -z "${TALEA_KS_PASS:-}" ]; then
        printf 'Keystore password (%s, alias %s): ' "$KEYSTORE" "$KEY_ALIAS" >&2
        read -rs TALEA_KS_PASS
        printf '\n' >&2
        [ -n "$TALEA_KS_PASS" ] || die "no password entered"
    fi
    export TALEA_KS_PASS
}

# ---- subcommands ------------------------------------------------------------
cmd_apk() {
    setup_env
    prompt_password
    ensure_frontend
    local bt; bt="$(latest_dir "$ANDROID_HOME/build-tools")"
    [ -n "$bt" ] || die "no build-tools under $ANDROID_HOME/build-tools"

    info "Building release APK…"
    cargo tauri android build --apk

    local unsigned aligned signed
    unsigned="$APK_DIR/app-universal-release-unsigned.apk"
    [ -f "$unsigned" ] || unsigned="$(find "$APK_DIR" -name '*-release-unsigned.apk' 2>/dev/null | head -1)"
    [ -f "$unsigned" ] || die "unsigned APK not found in $APK_DIR"
    aligned="$APK_DIR/app-universal-release-aligned.apk"
    signed="$APK_DIR/app-universal-release-signed.apk"

    info "Aligning + signing…"
    "$bt/zipalign" -f -p 4 "$unsigned" "$aligned"
    "$bt/apksigner" sign --ks "$KEYSTORE" --ks-key-alias "$KEY_ALIAS" \
        --ks-pass "env:TALEA_KS_PASS" --key-pass "env:TALEA_KS_PASS" \
        --out "$signed" "$aligned"
    "$bt/apksigner" verify "$signed" >/dev/null
    info "Signed APK ready. Install it with:"
    printf '\n  adb install -r "%s"\n\n' "$ROOT/$signed"
}

cmd_aab() {
    setup_env
    prompt_password
    ensure_frontend

    info "Building release AAB…"
    cargo tauri android build --aab
    [ -f "$AAB" ] || die "AAB not found at $AAB"

    info "Signing AAB (jarsigner)…"
    jarsigner -sigalg SHA256withRSA -digestalg SHA-256 \
        -keystore "$KEYSTORE" -storepass:env TALEA_KS_PASS -keypass:env TALEA_KS_PASS \
        "$AAB" "$KEY_ALIAS"
    jarsigner -verify "$AAB" >/dev/null

    mkdir -p "$DESKTOP"
    local base; base="$(app_name)-$(app_version)"
    cp -f "$AAB" "$DESKTOP/$base.aab"
    info "→ $DESKTOP/$base.aab"
    if [ -f "$MAPPING" ]; then
        cp -f "$MAPPING" "$DESKTOP/$base-mapping.txt"
        info "→ $DESKTOP/$base-mapping.txt"
    else
        info "no R8 mapping.txt found — skipped"
    fi
}

cmd_symbols() {
    setup_env
    ensure_frontend

    # Keep the native symbol table (override the profile's `strip = true` for this
    # build only — AGP still strips the *packaged* libs, so shipped size is
    # unaffected; only the jniLibs we zip retain symbols).
    info "Building native libs with symbols (CARGO_PROFILE_RELEASE_STRIP=false)…"
    CARGO_PROFILE_RELEASE_STRIP=false cargo tauri android build --apk
    [ -d "$JNILIBS" ] || die "jniLibs not found at $JNILIBS"

    mkdir -p "$DESKTOP"
    local base out; base="$(app_name)-$(app_version)"; out="$DESKTOP/$base-native-symbols.zip"
    rm -f "$out"
    # zip dereferences the jniLibs symlinks and stores the real (unstripped) .so.
    ( cd "$JNILIBS" && zip -r "$out" ./*/*.so >/dev/null )
    info "→ $out"
}

usage() {
    sed -n '3,23p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

case "${1:-}" in
    apk)     cmd_apk ;;
    aab)     cmd_aab ;;
    symbols) cmd_symbols ;;
    release) cmd_aab; cmd_symbols ;;
    -h|--help|help) usage 0 ;;
    *) printf 'unknown subcommand: %s\n\n' "${1:-<none>}" >&2; usage 1 ;;
esac
