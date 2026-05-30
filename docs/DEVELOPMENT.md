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

### Run it

```bash
# Over the LAN — most reliable for a physical device. Pass your machine's LAN IP:
just android-dev-host 192.168.1.20

# Or over USB (uses adb reverse to map the device's localhost to the host):
just android-dev

# Release APK/AAB (under src-tauri/gen/android):
just android-build
```

The first run downloads Gradle dependencies and is slow; later runs are quick
and hot-reload the frontend.

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
2. **Prefer the LAN host path.** Run `just android-dev-host <your-LAN-IP>` and
   make sure the phone and PC are on the **same network** and the host firewall
   allows the port. This avoids relying on `adb reverse`.
3. **If using USB (`just android-dev`),** confirm the reverse tunnel exists:

   ```bash
   adb reverse --list                      # expect: ... tcp:1420 tcp:1420
   adb reverse tcp:1420 tcp:1420           # (re)create it if missing, then relaunch
   ```

4. **Confirm Vite is serving** — the `cargo tauri android dev` terminal should
   show `VITE ready` and a local URL.
5. **HMR over LAN.** The dev CSP is `connect-src 'self'`; the HMR socket on a
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
