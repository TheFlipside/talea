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
and install a release package. This also sidesteps the firewall/networking
entirely, so it's a good way to confirm whether a blank screen is a dev-server
connection problem:

```bash
just android-build
adb install -r \
  src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

(If the packaged app renders but `android-dev*` doesn't, the issue is the dev
server connection — see Troubleshooting. The exact APK path can vary by
target/flavour; check the `cargo tauri android build` output.)

### Testing the biometric app lock

1. In the app: **cog → Settings → enable "Require biometric unlock."**
2. The lock applies on the **next launch** (so enabling it can't strand you
   behind a prompt you cancel). Fully close and reopen the app:

   ```bash
   adb shell am force-stop app.talea.budget   # then relaunch from the launcher
   ```

3. On relaunch the system biometric prompt appears → authenticate (fingerprint /
   face, or **Use PIN**) → the app unlocks. Cancel it to confirm you stay on the
   lock screen with the **Unlock** retry button.

Reset on-device state (database **and** the lock preference) for a clean first
run:

```bash
just android-reset       # → adb shell pm clear app.talea.budget
```

---

## Troubleshooting

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
