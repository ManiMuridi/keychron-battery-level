# Keychron Battery

A native macOS menu bar app written in Rust. AppKit draws the menu and status item;
IOKit reads standard HID battery fields. There is no webview or Swift runtime bridge.

## Download for macOS

**[Download the latest macOS release](https://github.com/ManiMuridi/keychron-battery-level/releases/latest)**

Download the **`.dmg`** from the release's **Assets** section. The universal app
supports **Apple Silicon and Intel Macs running macOS 13 or later**. You do not
need Rust, Xcode, or Homebrew to run it.

1. Open the downloaded `.dmg`.
2. Drag **Keychron Battery** into **Applications**.
3. Open **Keychron Battery** from Applications.
4. Click its keyboard icon in the menu bar, then **Enable Battery Access…**.
5. Enable it in **System Settings → Privacy & Security → Input Monitoring**.
   Quit and reopen the app if macOS asks you to.

The app is locally signed, but not Apple-notarized. If macOS blocks the download,
use **System Settings → Privacy & Security → Open Anyway** after attempting to
open it. A `.zip` download and SHA-256 checksums are also available on the release page.

## Build and run

Requires macOS 13+, Apple's Command Line Tools (`xcode-select --install`), and Rust
installed through [rustup](https://rustup.rs). Build on the architecture you want to run.

```sh
source "$HOME/.cargo/env"
./scripts/build-app.sh
open "dist/Keychron Battery.app"
```

To build a universal release for Apple Silicon and Intel:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/package-release.sh
```

The resulting `.app` contains a standalone compiled executable. Rust is needed to
build it, not to run it. The build script signs it locally with an ad-hoc signature;
it is not notarized for public distribution.

Click the keyboard icon in the menu bar, then **Enable Battery Access…**. In
**System Settings → Privacy & Security → Input Monitoring**, enable **Keychron
Battery**. If macOS requests a restart, quit and reopen the app. Otherwise click
**Refresh Battery**. Access cannot be granted by the app itself.

The app requests a fresh battery report every 30 seconds on a background thread.
Only one read is allowed at a time; a slow response does not block the menu. After
10 seconds without a response, any old percentage is hidden until the read recovers.
Its menu includes per-device readings,
manual refresh, a diagnostics copy action, Bluetooth settings, and Quit. It does not
install a login item; you can add the built app in System Settings → General → Login
Items if you want it to launch at login.

Launching with `cargo run` works for UI development, but use the `.app` when testing
permissions. After rebuilding an ad-hoc-signed binary, macOS may require you to
remove/re-add it under Input Monitoring. Keep the app at a consistent path.

## Battery unavailable after unlocking or updating macOS

Input Monitoring can be **granted** while macOS still refuses battery reads with
`kIOReturnNotPermitted` (`0xe00002e2`). One cause is **Secure Input**, which protects
the keyboard interface while a password field or the login screen is active.
It can remain active after unlocking the Mac.

The app detects this state and shows **Battery reads paused by macOS Secure Input**.
It pauses HID reads and automatically retries when Secure Input ends. Use **Secure
Input Help…** for recovery steps:

1. Finish entering passwords and close any password dialogs.
2. If the message remains after unlocking, lock the Mac with **Control–Command–Q**
   and unlock it again.
3. If it persists, save your work and restart macOS.

The app checks the state using Apple's `IsSecureEventInputEnabled` API; it never
disables Secure Input. See Apple's [Secure Event Input technical note](https://developer.apple.com/library/archive/technotes/tn2150/_index.html).

If Secure Input is off but macOS still blocks the read, choose **Review Battery
Access…** and check Input Monitoring. After replacing an ad-hoc-signed build, remove
and re-add the copy in Applications, then quit and reopen it. Recompiling alone does
not clear an active Secure Input session or restore a changed permission identity.

## What can it read?

The app looks for connected HID devices whose product name contains
`Keychron`, including older keyboards that identify with Apple's vendor ID in Mac
mode. It reads only these input usages:

- Generic Device Controls / Battery Strength (`0x06 / 0x20`).
- Battery System / Absolute State of Charge (`0x85 / 0x65`).

The connected K2 tested during development declares Battery Strength in input
report 3, with logical range 0–100. A standard input-report read returned **100%**
with a valid timestamp, also verified in the live menu bar. This
keyboard identifies as vendor `0x05AC`, product `0x024F`, firmware `1.0.9`.
Other models and firmware versions still need hardware testing.

The initial cached-only read was empty. The working path is
`IOHIDDeviceGetValueWithOptions` with `kIOHIDDeviceGetValueWithUpdate`, requesting
the report described by the standard battery element. No model-specific command
bytes or firmware changes are involved.

The app shows **— / Unavailable** when it has no valid measurement. A missing
timestamp, invalid range, or out-of-range value is never presented as a percentage.
An actual measured zero is shown as 0%. The displayed percentage is the keyboard's
estimate; it does not predict remaining hours or imply 1% measurement accuracy.
If the firmware supplies no value, use **Fn+B** on the keyboard for its own estimate.

This version does not read BLE GATT batteries, change firmware, or send speculative
vendor commands. Newer Keychron models that expose only a BLE Battery Service would
need a CoreBluetooth backend.

## Privacy and permissions

macOS gates access to the keyboard's HID interface behind Input Monitoring. The app
requests it only from **Enable Battery Access…**. It does not record keystrokes,
register keyboard event callbacks, send output reports, use the network, or save
telemetry. It enumerates device metadata and only reads the listed battery usages
from matching keyboards, using standard HID GET_REPORT requests. Handles are opened without seizing the keyboard and are
closed after each refresh.

**Copy Diagnostics** copies the current snapshot as JSON. It includes product name,
transport, vendor/product IDs, app version, permission and Secure Input states,
battery field metadata and errors;
it omits Bluetooth addresses, serial numbers, and keyboard input.

For a one-time terminal diagnostic:

```sh
cargo run -- --diagnose
# To inspect only the cached value without requesting a report:
cargo run -- --cached
# Or:
"dist/Keychron Battery.app/Contents/MacOS/keychron-battery" --diagnose
```

macOS can attribute terminal-launched processes to the terminal's permission
identity. For the running app's actual permission state, use its **Copy Diagnostics**.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- `src/app.rs`: native status item, menu, timer, permission flow.
- `src/hid.rs`: small documented C FFI surface for IOKit/Core Foundation.
- `src/model.rs`: validated readings and display state.
- `scripts/build-app.sh`: release build, app bundle, local signing.
- `scripts/package-release.sh`: universal DMG, ZIP, and SHA-256 checksums.

Pushing a version tag such as `v0.1.1` runs the release workflow, validates the tag
against `Cargo.toml`, and publishes prebuilt downloads. Update the version in
`Cargo.toml` and `Cargo.lock` before tagging. The app bundle version is derived from
Cargo metadata during packaging.

Manual checks: permission denied, grant and restart, keyboard disconnect/reconnect,
valid/absent battery reading, Secure Input pause and recovery, refresh, copy
diagnostics, and quit.

## Reference

Inspired by [rxrdev/keychron-battery-level](https://github.com/rxrdev/keychron-battery-level),
reviewed at commit `c95857a1ed2525da6846b9270c67ad7d6de5e7c7` (MIT, copyright 2025 Razvan).
That app uses CoreBluetooth and HID. This implementation focuses on the older K2's
standard HID battery usage and is written independently in Rust. See
`THIRD_PARTY_NOTICES.md` for the reference project's license notice.

Dependencies have their own licenses, recorded in Cargo metadata and `Cargo.lock`.
