# Android apps

Some services have no API and no usable website: only a mobile app. For those,
autocruise drives the **official app**, logged in with **your own account**, on
an Android device **you control**. It taps and reads the screen as you would.

```
connector (e.g. a future tgtg) ──▶ autocruise-android::Device ──adb──▶ Android device ──▶ official app
```

## Rules

These extend [PRINCIPLES.md](PRINCIPLES.md) (rules 3 and 4):

- **Official, unmodified app.** Install it from the publisher. When the APK does
  not come from Google Play directly, check its signing certificate against the
  publisher's (`apksigner verify --print-certs`) before installing.
- **The user handles login and challenges** on the device screen (`scrcpy`).
  If the app refuses to run, or a verification or block page appears, the
  connector stops and returns `user_action_required`. It never retries in a loop.
- **Human pace.** `Device` throttles every input (600 ms minimum by default).
- **Confirmation for side effects**, as everywhere else. No payment.

## Device options

| Option | When |
|---|---|
| **redroid** (default) | Android in a Docker container on the autocruise machine. Headless, scriptable, adb over TCP. |
| Your own phone | Most realistic; plug it in with USB debugging and set `AUTOCRUISE_ANDROID_SERIAL`. |
| Waydroid | Android in a Linux desktop session. Fine on a desktop, awkward headless. Not used by default. |
| Android SDK emulator | Heavier (full VM). Use if the others do not fit. |

redroid was chosen as the default because it is headless, reproducible
(`docker compose`), exposes adb on `127.0.0.1`, and runs ARM apps on x86 hosts
through its native bridge.

Some services refuse emulators or flag server IP addresses. Running at home,
or on a server with [egress through your home connection](EGRESS.md), fixes the
IP part. Beyond that it is the service's decision: try your own phone, or give
up on that service. Never disguise the device.

## Setup (redroid)

Requirements: Linux with the `binder_linux` kernel module (Ubuntu ships it),
Docker, and `adb` (`apt install adb`).

```bash
sudo modprobe binder_linux                      # once per boot
docker compose -f deploy/android/compose.yaml up -d
adb connect 127.0.0.1:5555
adb -s 127.0.0.1:5555 shell getprop sys.boot_completed   # 1 when ready
```

To load binder at every boot: `echo binder_linux | sudo tee /etc/modules-load.d/autocruise-binder.conf`.

Install an app (split APKs: base + ABI + language + density):

```bash
apksigner verify --print-certs base.apk         # compare with the publisher's certificate
adb -s 127.0.0.1:5555 install-multiple base.apk config.armeabi_v7a.apk config.fr.apk config.xhdpi.apk
```

Log in yourself, on the device screen: `scrcpy -s 127.0.0.1:5555`.

Configuration: `AUTOCRUISE_ANDROID_SERIAL` (default `127.0.0.1:5555`) and
`AUTOCRUISE_ADB` (default `adb`).

## Writing an app connector

The connector depends on `autocruise-android` and keeps its selectors
(resource ids, texts) and flows to itself. To explore an app:

```bash
cargo run -p autocruise-android --example android_ui                          # print the screen
cargo run -p autocruise-android --example android_ui -- tap-text "Continue"   # tap, then print
cargo run -p autocruise-android --example android_ui -- back
```

Prefer resource ids over texts (texts change with the language). Record UI
dumps as fixtures under `connectors/<name>/tests/fixtures/`, **with personal
data removed**, and test against `autocruise_android::mock::MockAdb`.

Every app connector must detect its service's block and challenge screens
and map them to `user_action_required`.

## Service status

| Service | Result | Date |
|---|---|---|
| Too Good To Go | **Blocked.** The official app (26.10.0, signature checked) starts on redroid, but its first network screen shows DataDome's "Access is temporarily restricted / unusual activity from your device or network". Retried once through the user's home connection ([EGRESS.md](EGRESS.md), residential IP verified): same block, so the device itself (emulator, adb) is flagged, not the IP. Not pursued on redroid. Remaining option: the user's own phone. | 2026-10-19 |
