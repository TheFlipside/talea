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

## Android

### Prerequisites (one-time)

- **JDK 17.** Installed here via `sudo apt install openjdk-17-jdk`, giving
  `JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64`. (Gradle/AGP want JDK 17.)
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

## iOS (App Store)

iOS builds require **macOS + Xcode**. `cargo tauri ios init` generates the
native project under `src-tauri/gen/apple` (gitignored, like `gen/android`), and
`cargo tauri ios build` produces the archive to upload via Xcode Organizer or
Transporter. The bundle identifier is `com.luminaapps.talea` (from
`tauri.conf.json`).

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
- The WidgetKit **extension** is a separate Xcode target. Because Tauri can't
  generate it, its sources live in [`ios-widget/`](../ios-widget/README.md) and
  are added to the generated Xcode project on macOS. Follow
  `ios-widget/README.md`: enable the App Group on both the app and widget targets,
  add the `TaleaWidget` target (iOS 17+), and add the source files. Re-apply after
  any `cargo tauri ios init`.

---

## Troubleshooting

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
