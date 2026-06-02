# Talea — Development & device testing

Practical setup for building Talea on the desktop (day-to-day dev) and running
it on a physical **Android** device, including how to exercise the biometric
app lock. See `README.md` for the project overview and `docs/DESIGN.md` for the
design decisions.

> The generated native project under `src-tauri/gen/android/` is **gitignored**
> on purpose — it's reproducible from `cargo tauri android init` and shouldn't be
> committed.

---

## Desktop (primary dev loop)

```bash
just install      # one-time: install frontend deps
just dev          # cargo tauri dev — Vite + the desktop shell
just gate         # full pre-commit gate (lint + tests + builds)
```

Biometrics are mobile-only; on the desktop build the lock setting persists but
never engages (there's no authenticator), so the app always opens.

---

## Quality gates

All gates must pass clean (also wrapped as `just gate`):

```bash
cargo clippy --workspace --all-targets -- -W clippy::pedantic -D warnings
cargo fmt --all --check
cargo test --workspace
npm --prefix frontend run lint     # eslint --max-warnings=0
npm --prefix frontend run build    # tsc + vite build
npm --prefix frontend test         # vitest (incl. the locale key-parity test)
```

### sqlx offline cache

SQL in `src-tauri` is compile-time checked by `sqlx::query!` against the
committed `.sqlx/` cache (`SQLX_OFFLINE=true` in `.cargo/config.toml`), so the
gates build with **no database**. After changing any query or migration,
regenerate the cache and commit it:

```bash
# one-time: a matching sqlx-cli
cargo install sqlx-cli --version ^0.9 --no-default-features --features sqlite
# regenerate .sqlx against a scratch DB migrated from src-tauri/migrations
export DATABASE_URL="sqlite:///tmp/talea-prepare.sqlite3"
sqlx database create && sqlx migrate run --source src-tauri/migrations
cargo sqlx prepare --workspace          # then commit the updated .sqlx/
```

Verify the committed cache still matches the code with
`SQLX_OFFLINE=false cargo sqlx prepare --workspace --check` against the same
`DATABASE_URL`.

---

## Resetting local data

The app stores its SQLite database in the OS app-data directory under the
identifier `com.luminaapps.talea`. Deleting it gives a clean first run (which
auto-creates a default account):

```bash
# Linux
rm -f ~/.local/share/com.luminaapps.talea/talea.sqlite3*
# macOS
rm -f ~/Library/Application\ Support/com.luminaapps.talea/talea.sqlite3*
# Windows (PowerShell)
Remove-Item "$env:APPDATA\com.luminaapps.talea\talea.sqlite3*"
```

The `talea.sqlite3*` glob also removes the `-wal`/`-shm` WAL sidecar files. The
Nextcloud credentials (`nextcloud.json`) live in the same directory and are
**not** removed by that glob — delete it too for a fully clean slate.

---

## Android

### Prerequisites (one-time)

- **JDK 17 (or 21) — and `JAVA_HOME` must point at it.** Installed here via
  `sudo apt install openjdk-17-jdk`, giving
  `export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64`. Gradle/AGP and the
  Kotlin compiler that builds the generated `buildSrc` only support up to JDK 21.
  If `JAVA_HOME` is unset, Gradle uses the system-default `java` — and a newer one
  (e.g. JDK 25) fails at *configure* time with a cryptic
  `A problem occurred configuring project ':buildSrc'. > <version>`
  (`IllegalArgumentException` from `JavaVersion.parse`). Fix: export `JAVA_HOME`
  to a 17/21 JDK before building (Android Studio's bundled JBR at
  `~/Dev/android-studio/jbr` is JDK 21 and also works).
- **Android SDK + NDK.** Provided by an Android Studio install. Use the SDK
  Manager to ensure an *SDK Platform* (API 34/35), *Platform-Tools*, and the
  *NDK* are present. The SDK lives at `~/Android/Sdk` by default.
- **Rust Android targets:**

  ```bash
  rustup target add aarch64-linux-android armv7-linux-androideabi \
                    i686-linux-android x86_64-linux-android
  ```

  (A modern physical device needs `aarch64`; an emulator usually `x86_64`.)

### Environment

The `just` Android recipes set these for you, defaulting `JAVA_HOME` to the apt
OpenJDK path and `ANDROID_HOME` to `~/Android/Sdk`, and auto-detecting the latest
installed NDK. Override any of them via the environment. To set them in your own
shell instead:

```bash
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
```

### Initialize the native project (one-time)

```bash
just android-init        # → cargo tauri android init, creates src-tauri/gen/android
```

`cargo tauri android init` scaffolds the project with the **default Tauri
launcher icon**, so the recipe immediately reapplies the branded icons via
`cargo tauri icon src-tauri/icons/icon-manifest.json` (adaptive icon: the
ring/calendar foreground on the dark `#122E38` tile). If you ever run
`cargo tauri android init` directly, run that icon command afterwards or the APK
ships the stock Tauri logo.

### Connect the device

1. On the phone: enable **Developer options → USB debugging**.
2. Plug in via USB; run `adb devices` and accept the authorization prompt.
3. **Set up a screen lock and enrol a fingerprint/face** (or at least a device
   PIN). This matters for the app lock: where no authenticator is available the
   app deliberately does **not** lock (so you can't get stranded), so without
   enrolment you'd never see the prompt.

### Run it (live dev server)

```bash
# Over the LAN — most reliable for a physical device. Pass your machine's LAN IP:
just android-dev-host 192.168.1.20

# Or over USB (uses adb reverse to map the device's localhost to the host):
just android-dev
```

> **Firewall (important):** in LAN mode the device connects to the Vite dev
> server on your machine, so a host firewall will block it and you get a blank
> white screen. On Ubuntu with `ufw` active, allow the ports:
>
> ```bash
> sudo ufw allow 1420/tcp     # Vite dev server
> sudo ufw allow 1421/tcp     # Vite HMR (live reload)
> ```
>
> This was the cause of the first white screen during bring-up. Revoke later
> with `sudo ufw delete allow 1420/tcp` (and `1421/tcp`) if you prefer.

The first run downloads Gradle dependencies and is slow; later runs are quick
and hot-reload the frontend.

### Build & sign with `scripts/android.sh` (recommended)

`scripts/android.sh` automates the build → align → sign → collect steps so you
don't run them by hand. It auto-detects the NDK and build-tools, forces a JDK
≤ 21, prompts **once** for the keystore password (then signs unattended), and
names release artifacts `<App>-<version>` on your Desktop.

```bash
just android-apk        # build + sign a test APK, then print the adb install line
just android-aab        # build + sign the Play .aab + copy it & mapping.txt to ~/Desktop
just android-symbols    # build native (Rust) symbols → ~/Desktop/<App>-<ver>-native-symbols.zip
just android-release    # aab + symbols together (full Play upload set)
# or call the script directly: ./scripts/android.sh {apk|aab|symbols|release}
```

Signing inputs are overridable via the environment (defaults in parentheses):
`TALEA_KEYSTORE` (`~/play-store_release-key.keystore`), `TALEA_KEY_ALIAS`
(`play-store_release`), `TALEA_DESKTOP` (`~/Desktop`). The password is read with
`read -rs` and passed to `apksigner`/`jarsigner` via the environment, never on the
command line. `apk`/`release` leave the signed APK under `gen/android/...` and
print its `adb install -r` command. The manual steps below document what the
script does, for reference or one-off tweaks.

### Run it (standalone APK — no dev server)

To run without a live dev server (the frontend is bundled into the APK), build
and install a package. This also sidesteps the firewall/networking entirely, so
it's a good way to confirm whether a blank screen is a dev-server connection
problem.

**Debug (simplest — auto-signed with the debug key, installs directly):**

```bash
cargo tauri android build --debug
adb install -r \
  src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

**Release (unsigned — must be signed before it will install):** a release build
has no Gradle signing config, so it emits `app-universal-release-unsigned.apk`.
Align, then sign with your keystore, then install (`$BT` = your build-tools dir,
e.g. `$ANDROID_HOME/build-tools/37.0.0-rc2`):

```bash
BT="$ANDROID_HOME/build-tools/37.0.0-rc2"
OUT="src-tauri/gen/android/app/build/outputs/apk/universal/release"
just android-build                                   # → app-universal-release-unsigned.apk
"$BT/zipalign" -f -p 4 "$OUT/app-universal-release-unsigned.apk" "$OUT/app-universal-release-aligned.apk"
"$BT/apksigner" sign --ks <keystore> --ks-key-alias <alias> \
  --out "$OUT/app-universal-release-signed.apk" "$OUT/app-universal-release-aligned.apk"
adb install -r "$OUT/app-universal-release-signed.apk"
```

`zipalign` must run **before** `apksigner` (the signer preserves alignment, it
doesn't realign). `apksigner` prompts for the keystore/key passwords — don't pass
them on the command line. If a build signed with a different key (e.g. the debug
APK) is already installed, `adb install` fails with
`INSTALL_FAILED_UPDATE_INCOMPATIBLE`; `adb uninstall com.luminaapps.talea` first.

(If the packaged app renders but `android-dev*` doesn't, the issue is the dev
server connection — see Troubleshooting. The exact APK path can vary by
target/flavour; check the `cargo tauri android build` output.)

### Play Store release bundle (`.aab`)

The Play Store takes an **App Bundle** (`.aab`), not an APK. `cargo tauri android
build --aab` emits an **unsigned** bundle (the project has no Gradle signing
config) at:

```
src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab
```

An `.aab` is a JAR-format archive, so it is signed with **`jarsigner`** — *not*
`apksigner`/`zipalign`, which are APK-only (Google generates and aligns the APKs
from your bundle).

```bash
# One-time: create an upload keystore. Back it up — it's your upload key forever.
keytool -genkeypair -v -keystore ~/talea-upload.jks \
  -alias upload -keyalg RSA -keysize 2048 -validity 10000

AAB="src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab"
cargo tauri android build --aab
jarsigner -sigalg SHA256withRSA -digestalg SHA-256 -keystore ~/talea-upload.jks "$AAB" upload
jarsigner -verify "$AAB"          # → "jar verified."
```

`jarsigner` signs in place and prompts for the password (don't pass it on the
command line). With **Play App Signing** (default for new apps) this is only your
*upload* key — Google re-signs with the app-signing key it holds — so it just has
to stay consistent across uploads. `versionCode` must increase on every upload;
Tauri derives it from the app version (override via `bundle.android.versionCode`
in `tauri.conf.json` if Play reports a clash).

> `src-tauri/gen/android/` is regenerated and not committed, so don't add a
> `signingConfig` to `build.gradle.kts` (it would be wiped). The post-build
> `jarsigner` step is the durable approach.

#### Optional Play uploads: deobfuscation mapping + native debug symbols

Both are optional but make Play Console crash reports readable — they turn a
native panic (like the rustls one fixed in 1.4.1) from raw addresses into named
frames.

- **R8 mapping (the Java/Kotlin layer).** The release build minifies
  (`isMinifyEnabled = true`), so a mapping file is already produced at:

  ```
  src-tauri/gen/android/app/build/outputs/mapping/universalRelease/mapping.txt
  ```

  Upload it in the Play Console for the release (app-bundle explorer → Downloads →
  ReTrace mapping file); it's also bundled automatically when present at build.

- **Native debug symbols (the Rust `.so`).** Not produced by default: the release
  profile strips them (`[profile.release] strip = true` in the workspace
  `Cargo.toml`, and the `jniLibs` libraries symlink straight to the stripped
  `target/<abi>/release/` output). Produce a symbol-bearing build by disabling
  stripping for one build, then zip the per-ABI libraries:

  ```bash
  # Temporarily set `strip = false` under [profile.release] in Cargo.toml, then:
  cargo tauri android build --aab
  ( cd src-tauri/gen/android/app/src/main/jniLibs && \
    zip -r ~/talea-native-symbols.zip ./*/*.so )   # arm64-v8a/…, armeabi-v7a/…, x86/…
  # Revert to `strip = true` afterwards so shipped builds stay small.
  ```

  Upload `talea-native-symbols.zip` in the Play Console (app-bundle explorer →
  Downloads → Native debug symbols). `strip = false` keeps the **symbol table**
  (function names — enough to symbolicate a stack trace); also add `debug = true`
  to the profile if you want source line numbers too (much larger output). The
  *shipped* bundle is unaffected — AGP strips the packaged libraries during
  release packaging; only the `jniLibs` source you zip retains the symbols.
  (`zip` dereferences the symlinks and stores the real `.so` contents.)

### Testing the biometric app lock

1. In the app: **cog → Settings → enable "Require biometric unlock."**
2. The lock applies on the **next launch** (so enabling it can't strand you
   behind a prompt you cancel). Fully close and reopen the app:

   ```bash
   adb shell am force-stop com.luminaapps.talea   # then relaunch from the launcher
   ```

3. On relaunch the system biometric prompt appears → authenticate (fingerprint /
   face, or **Use PIN**) → the app unlocks. Cancel it to confirm you stay on the
   lock screen with the **Unlock** retry button.

Reset on-device state (database **and** the lock preference) for a clean first
run:

```bash
just android-reset       # → adb shell pm clear com.luminaapps.talea
```

### System bar appearance (status / navigation bar icons)

The app draws edge-to-edge, so the OS status/navigation bar icons sit over the
header. Their colour is a **native** setting (not controllable from CSS/JS).
Talea handles it with an in-tree Tauri plugin, **`tauri-plugin-statusbar`**: the
frontend calls it whenever the resolved theme changes
([`lib/statusbar.ts`](../frontend/src/lib/statusbar.ts), from the theme effect in
`AppProviders`), and the plugin sets the bar icons to match — light icons in
Talea's dark theme, dark icons in light. This tracks **Talea's** theme, so it's
correct even when the device's own light/dark setting differs.

- Rust: `tauri-plugin-statusbar/src/lib.rs` (a `set_dark` command; no-op on
  desktop). Android: `WindowInsetsControllerCompat` light-bar flags. iOS: the
  window's `overrideUserInterfaceStyle`.
- It's a normal committed crate (outside `gen/`), wired in `src-tauri`
  (`tauri_plugin_statusbar::init()`) with the `statusbar:default` capability.

The generated `values/themes.xml` + `values-night/themes.xml` may also set
`android:windowLightStatusBar` / `windowLightNavigationBar` (true in day, false
in night) as a sensible default for the brief moment before the WebView loads,
but the plugin is the authority once the app is running.

### Home-screen widget

The abstract budget-ring widget (DESIGN.md §6) is provided by
`tauri-plugin-budgetwidget`. Its Android side is an `com.android.library`
(`tauri-plugin-budgetwidget/android`) whose `AndroidManifest.xml` declares the
`AppWidgetProvider` receiver + config activity; those **merge into the app
manifest**, so the launcher lists the widget under `com.luminaapps.talea` — no
edits to the generated `gen/android` project are needed.

To test on a device:

1. Build/run the app (`just android-dev` / `just android-dev-host <ip>`) and open
   it at least once so it publishes a snapshot (the plugin writes the abstract
   per-account ring fraction to `SharedPreferences "talea_widget"`).
2. Long-press the home screen → **Widgets** → Talea → drag the widget out. The
   config screen lists your accounts; pick one. The ring + percentage render.
3. Record an entry or switch accounts in-app: the widget redraws (the app pushes
   updates; it does not poll). Tapping the widget opens the app.
4. To **change the tracked account** after placement (Android 12+/API 31), the
   widget is `reconfigurable`: long-press it → tap the reconfigure (pencil)
   affordance to reopen the account picker. On older Android, remove and re-add.
5. Only the ring/percentage/name are shown — no amounts — so it's safe on the
   lock screen / while the app is biometric-locked.

---

## iOS (macOS + Xcode)

iOS builds require **macOS + Xcode** and an Apple Developer team. The native
project under `src-tauri/gen/apple` is **gitignored and regenerated from
`project.yml` (XcodeGen) on every cli build** — so configure everything through
`tauri.conf.json` / `project.yml` / the icon manifest, **never the Xcode UI**
(GUI edits, manually-added targets, and "Signing & Capabilities" tweaks are
wiped on the next build). The bundle id is `com.luminaapps.talea`; the app
category, version, and display name come from `tauri.conf.json`.

> **Drive every build through the tauri-cli, not Xcode's Run button.** The Rust
> build phase (`cargo tauri ios xcode-script`) connects back to the running cli
> over a WebSocket; pressing Run in Xcode standalone fails with
> `failed to build WebSocket client … Connection refused`.

### One-time setup

1. **Development team.** Set it once so signing works headlessly (no Xcode team
   picker). Either add to `tauri.conf.json` under `bundle`:
   ```json
   "iOS": { "developmentTeam": "XXXXXXXXXX" }
   ```
   or `export APPLE_DEVELOPMENT_TEAM=XXXXXXXXXX` in your shell (the `just ios-*`
   recipes inherit it). Find the 10-char Team ID in the Apple Developer portal →
   Membership.
2. **Distribution certificate** (App Store builds only; needs the paid Apple
   Developer Program). Xcode → Settings → Accounts → your Apple ID → **Manage
   Certificates → + → Apple Distribution**. A development cert alone yields an
   `Apple Development`-signed IPA that App Store Connect rejects.
3. **App Group** (required for the widget — without it the widget shows no data
   and its account picker is empty). The app and the widget share data through
   the App Group container; if it isn't **registered in the portal**,
   `UserDefaults(suiteName:)` is nil on both sides and nothing is shared.
   1. Apple Developer portal → **Certificates, Identifiers & Profiles →
      Identifiers → App Groups → +** → register the identifier
      **`group.com.luminaapps.talea`**.
   2. Build with `just ios-release` (automatic signing then enables the App
      Groups capability on both App IDs — `com.luminaapps.talea` and
      `com.luminaapps.talea.TaleaWidget`, creating the latter — and adds it to
      the provisioning profiles). The entitlement files already declare the
      group (added by `configure_ios_project.py`).
   3. Open the app once so it publishes a snapshot, then the widget/picker
      populate. If they stay empty, see Troubleshooting → "Widget shows no data".
4. **Tooling for the recipes:** `python3 -m pip install Pillow pyyaml` and
   `brew install xcodegen` (xcodegen is already an iOS prerequisite). `ios-init`
   uses them to configure the project and icons.
5. **Generate + configure the project:**
   ```bash
   just ios-init
   ```
   `ios init` regenerates `gen/apple` from a template (and scaffolds the **default
   Tauri icon**), so the recipe then:
   - runs `scripts/configure_ios_project.py` — patches `project.yml` to add
     `NSFaceIDUsageDescription` (without it iOS reports Face ID unavailable and
     the lock silently disengages), the App Group on the app entitlements, and
     the **`TaleaWidget`** app-extension target (sources in `ios-widget/`,
     embedded in the app), then regenerates the `.xcodeproj` with `xcodegen`;
   - reapplies the branded icon (`cargo tauri icon`) and strips the iOS icons'
     alpha channel (`scripts/flatten_ios_icons.py`) — App Store rejects a 1024px
     marketing icon with alpha even when opaque.

   Re-run `just ios-init` after any change to `ios-widget/` or if the icon shows
   the stock Tauri logo.

### Live testing on a device

```bash
just ios-dev        # cargo tauri ios dev — builds, signs, installs, live-reloads
```

Keep the terminal running (it hosts the dev server the app and the Xcode build
phase talk to). The device and Mac must be on the same network and able to reach
the dev host (a restrictive Wi-Fi/AP-isolation or firewall setup can block it,
like the Android LAN case below — `cargo tauri ios dev --host <mac-LAN-ip>` can
help).

### Building a publishable App Store archive

No Xcode "Product → Archive" needed — the cli produces the **distribution**-signed
IPA. Use the recipe (it passes the export method); a **bare `cargo tauri ios
build` exports a *development*-signed IPA** that App Store Connect rejects:

```bash
just ios-release    # cargo tauri ios build --export-method app-store-connect
# → src-tauri/gen/apple/build/arm64/Talea.ipa
```

**Verify the signature before uploading** (note: `codesign` on the `.ipa` itself
reports "not signed at all" — an IPA is just a zip; verify the `.app` inside):

```bash
# from the repo root:
unzip -o src-tauri/gen/apple/build/arm64/Talea.ipa -d /tmp/talea-ipa
codesign -dvvv /tmp/talea-ipa/Payload/Talea.app 2>&1 | grep Authority
# the first Authority= line is the leaf cert — expect:
#   Authority=Apple Distribution: <you> (<TEAMID>)
```

Then upload `Talea.ipa` to App Store Connect:

- **Transporter** (free, Mac App Store) — open it, sign in with your Apple ID,
  drag in the IPA (or **+**), and **Deliver**. This is the recommended path.
- CLI alternative: `xcrun iTMSTransporter` with an App Store Connect API key.
  (Note: `xcrun altool --upload-app` was removed in Xcode 16 — don't rely on it.)
  Keep any `.p8` API key out of source control (store it under
  `~/.appstoreconnect/private_keys/`).

The app record (matching bundle id `com.luminaapps.talea`) must already exist in
App Store Connect.

**Fallback — distribute via Xcode Organizer.** If the cli export can't create the
App Store provisioning profile headlessly, use the archive the build already
produced: double-click `src-tauri/gen/apple/build/talea_iOS.xcarchive` (opens in
Organizer) → **Distribute App → App Store Connect → Upload**. Xcode re-signs
using the Apple Distribution cert in your keychain and fetches/creates the App
Store profile. This runs no Rust build phase, so it sidesteps the PATH /
dev-server issues entirely.

#### Signing troubleshooting

- **`errSecInternalComponent` at CodeSign** (build from the cli): codesign can't
  reach the signing key non-interactively. In **Keychain Access** → login → My
  Certificates, expand the signing cert, double-click its **private key →
  Access Control** and add `/usr/bin/codesign` (preferred — minimal grant;
  "Allow all applications" also works but is a broader, less-restrictive grant).
  Also confirm the **Apple WWDR** intermediate cert is present and unexpired
  (reinstall from <https://www.apple.com/certificateauthority/>), and that the
  login keychain isn't auto-locking mid-build.
- **IPA signed `Apple Development` / Transporter "not a distribution cert"**: you
  ran the bare build (development export) — use `just ios-release`, and ensure the
  Apple Distribution cert from the setup step exists.
- **Transporter "Invalid large app icon … can't be transparent or contain an
  alpha channel"**: the iOS icons still have alpha. Run
  `python3 scripts/flatten_ios_icons.py` (needs Pillow) — `just ios-init` does
  this automatically — then rebuild.

> **Widget:** the WidgetKit extension (`ios-widget/`) is a **second target**,
> declared in `project.yml` by `scripts/configure_ios_project.py` (run from
> `just ios-init`) and embedded in the app, so `just ios-release` ships it. The
> App Group from the setup step must be registered in the portal or the
> extension won't sign.

### PATH note (only if you do build from Xcode)

GUI-launched Xcode doesn't inherit your shell `PATH`, so a Rust build phase run
from Xcode can't find `cargo`. Driving builds via the cli (above) avoids this. If
you must build in Xcode, add `source "$HOME/.cargo/env"` as the first line of the
"Build Rust Code" run-script phase (re-add after regeneration).

### Data protection (at-rest encryption)

Decision and rationale live in [`DESIGN.md` §9](DESIGN.md); this is the how-to.

**For v1: do nothing — the OS baseline is what we want.** iOS already encrypts
app-private files at rest at the
`NSFileProtectionCompleteUntilFirstUserAuthentication` class whenever the user
has a device passcode, so the SQLite DB is protected with no entitlement and no
code. The **Data Protection** capability
(`com.apple.developer.default-data-protection`, value `NSFileProtectionComplete`)
is the opt-in to the *stronger* class where files are **sealed while the device
is locked** — we deliberately avoid it for v1, because our `sqlx`/WAL connection
stays open and a sealed-when-locked file can raise `SQLITE_IOERR` if iOS
suspends the app around device lock. So you can leave Data Protection **off** on
the App ID for v1 with no loss of at-rest encryption.

**If you later want `Complete` sealing** (e.g. for a specific file, validated on
a physical device across lock/unlock with the DB open): enable the capability on
the App ID (Apple Developer → Identifiers → `com.luminaapps.talea` → Data
Protection), then after `cargo tauri ios init` add the entitlement to the app's
entitlements file (`gen/apple/Talea_iOS/Talea_iOS.entitlements` — match the
actual generated name), inside the top-level `<dict>`:

```xml
<key>com.apple.developer.default-data-protection</key>
<string>NSFileProtectionComplete</string>
```

Notes:

- This raises the **default** class for files the app creates. **Do not apply
  `NSFileProtectionComplete` to the SQLite DB or its `-wal`/`-shm` sidecars** —
  the open-connection caveat above applies there too. If you ever want the DB
  above the default, use `NSFileProtectionCompleteUnlessOpen` (keys stay valid
  while a file handle is open), set per-file via the file's protection
  attribute in code, and validate on a physical device.
- All of this only protects data when the user has a **device passcode** set;
  with no passcode the default class provides effectively no at-rest
  protection (consistent with the biometric-lock caveat in `DESIGN.md` §9).
- `gen/apple` is regenerated by `ios init`, so **re-apply this after any
  re-init** (this doc is the source of truth).
- The build's provisioning profile must include the Data Protection entitlement
  (it will, once the capability is enabled on the App ID).
- **Widget App Group:** the App Group container that publishes the abstract ring
  fraction (see below) must stay readable while the device is locked, so it must
  **not** be `Complete`-protected — leave it at the default level.

### Home-screen widget

The abstract budget-ring widget (DESIGN.md §6) has two iOS parts:

- The app-side bridge (`tauri-plugin-budgetwidget`) writes the abstract snapshot
  to an **App Group** (`group.com.luminaapps.talea`) and reloads timelines. It is
  built into the app automatically (no extra steps beyond enabling the App Group
  capability + entitlement on the app target).
- The WidgetKit **extension** is a separate Xcode target whose sources live in
  [`ios-widget/`](../ios-widget/README.md). Because `gen/apple` is regenerated
  from `project.yml` on every cli build, it can't be added via the Xcode UI (that
  gets wiped). Instead `scripts/configure_ios_project.py` (run by `just ios-init`)
  declares the `TaleaWidget` app-extension target in `project.yml` and embeds it
  in the app, so it persists and ships. The App Group must be registered in the
  portal (setup step 3) for the extension to sign.

---

## Backup & restore (Nextcloud over WebDAV)

Optional, manual backup/restore to the user's own Nextcloud (DESIGN.md §10).
Configured in-app under **Settings → Backup & sync**; nothing is set up at the
build level.

- **Server side:** in Nextcloud, create an **app password** under
  *Settings → Security → Devices & sessions* (do **not** use the login password).
  Backups land at `Talea/talea-backup.sqlite3` under the account's files; the
  `Talea/` folder is created on first backup (`MKCOL`).
- **Manual test loop:** enter the `https://` address + username + app password →
  *Save* → *Test connection* → *Back up now*; confirm the file appears in the
  Nextcloud web UI. On a second device (or after local edits), *Restore* and
  confirm the data matches. `http://` and bad credentials are rejected with a
  clear message; *Restore* is refused if the backup's schema version differs.
- **TLS / cross-compile:** networking is `reqwest` + rustls with the **`ring`**
  provider (not `aws-lc-rs`), so no OpenSSL or C/cmake crypto toolchain is needed
  on iOS/Android. To re-verify the Android cross-compile after a dependency bump,
  point the NDK clang at the target and build the lib only:

  ```bash
  NDK=~/Android/Sdk/ndk/<version>
  TC=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
  export CC_aarch64_linux_android=$TC/aarch64-linux-android24-clang \
         AR_aarch64_linux_android=$TC/llvm-ar \
         CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$TC/aarch64-linux-android24-clang
  cargo build -p talea --lib --target aarch64-linux-android
  ```

- **Credentials** are stored in `nextcloud.json` in the app-data directory,
  **outside** the database (so the password is never inside an uploaded backup)
  and are never returned to the frontend or logged. Resetting local data (see the
  README) does not remove `nextcloud.json`; delete it alongside the DB for a fully
  clean slate.

---

## Troubleshooting

### Widget shows no data / empty account picker (iOS)

Almost always the **App Group isn't registered** in the Apple Developer portal,
so `UserDefaults(suiteName: "group.com.luminaapps.talea")` returns nil for both
the app (can't write) and the widget (can't read) — the picker spins then shows
nothing and the ring is blank. Fix: register `group.com.luminaapps.talea` (iOS
setup step 3), rebuild with `just ios-release`, and open the app once. To
confirm the cause, watch device logs while the app launches — the publish step
logs `budget widget publish failed: … App Group … is unavailable` when the
group isn't provisioned.

### First build: Vite/`npm` not installed

A fresh clone has no `frontend/node_modules`, so a bare `cargo tauri dev/build`
fails when it runs the Vite `beforeDevCommand`. The `just` recipes
(`dev`, `build`, `test`, `lint`, `android-*`) depend on `_ensure-frontend`, which
runs `npm --prefix frontend install` only when it's missing — so `just dev` works
on a clean checkout. If you invoke `cargo tauri` directly, run `just install`
(or `npm --prefix frontend install`) once first.

### macOS/iOS build: "recursion limit reached" in `dispatch2`

A macOS/iOS build can fail compiling the Apple-only `dispatch2` crate:

```
error: recursion limit reached while expanding `$crate::__bitflags_flag_name!`
  --> .../dispatch2-0.3.1/src/generated/mod.rs
```

This is an upstream `bitflags` **2.12.0** regression: its `__bitflags_flag_name`
macro recurses once per flag attribute, and `dispatch2`'s flags carry long
doc-comments, so it overruns the default `recursion_limit` (128). `bitflags`
2.9.1 has no such macro. The committed `Cargo.lock` therefore **pins `bitflags`
to 2.9.1** (2.x line; the legacy 1.x is separate). If a future `cargo update`
reintroduces 2.12.0+ and the error returns, re-pin:

```bash
cargo update -p bitflags@2.12.0 --precise 2.9.1   # adjust the from-version
```

(Do not bump `bitflags` 2.x past 2.9.x until `dispatch2` ships a
`#![recursion_limit]` or the bitflags macro is fixed.)

### Blank / white screen on launch

This almost always means the device's WebView can't reach the Vite dev server.

1. **Inspect the WebView.** Open `chrome://inspect/#devices` in desktop Chrome
   with the device connected, click *inspect* on the Talea WebView, and look at
   the Console/Network tabs — that shows the real error (a refused connection to
   `:1420`, a CSP violation, or a JS error).
2. **Open the firewall** (the most common cause). In LAN mode the device must
   reach the dev server on your machine; `ufw` silently drops it:

   ```bash
   sudo ufw allow 1420/tcp    # Vite dev server
   sudo ufw allow 1421/tcp    # Vite HMR
   ```

3. **Prefer the LAN host path.** Run `just android-dev-host <your-LAN-IP>` with
   the phone and PC on the **same network**. This avoids relying on `adb reverse`.
4. **If using USB (`just android-dev`),** confirm the reverse tunnel exists:

   ```bash
   adb reverse --list                      # expect: ... tcp:1420 tcp:1420
   adb reverse tcp:1420 tcp:1420           # (re)create it if missing, then relaunch
   ```

5. **Confirm Vite is serving** — the `cargo tauri android dev` terminal should
   show `VITE ready` and a local URL.
6. **Still stuck?** Build a standalone APK (see "Run it (standalone APK)") — it
   bundles the frontend and needs no dev server, isolating networking issues.
7. **HMR over LAN.** The dev CSP is `connect-src 'self'`; the HMR socket on a
   separate port may be blocked, which breaks live-reload but not the initial
   render. A full re-run picks up changes if HMR is quiet.

### Watching logs

```bash
just android-log         # tails logs for the running app process
# or, broadly:
adb logcat | grep -iE "talea|RustStdoutStderr|chromium"
```

### Biometric prompt never appears (on a device with biometrics enrolled)

`cargo tauri android dev` should merge the biometric permission from the plugin.
If it doesn't, add to `src-tauri/gen/android/app/src/main/AndroidManifest.xml`:

```xml
<uses-permission android:name="android.permission.USE_BIOMETRIC"/>
```

(The `gen/android` tree is regenerated by `android init`; re-apply if you
re-init.)
