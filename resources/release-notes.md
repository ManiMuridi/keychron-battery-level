A native Rust menu bar battery monitor for Keychron keyboards on macOS.

### Fixed in 0.1.1

- Detect macOS Secure Input, which can block battery reads even when Input Monitoring is granted.
- Show a clear paused state with recovery instructions and automatically retry when Secure Input ends.
- Show HID read failures instead of incorrectly waiting for a battery report.
- Offer Input Monitoring settings when macOS denies a read despite a granted permission check.
- Include the app version and Secure Input state in diagnostics.

If Secure Input remains active after unlocking, try locking and unlocking again.
If that does not clear it, save your work and restart macOS. The app respects this
system protection; rebuilding cannot clear it.

### Download and install

1. Download the **universal `.dmg`** below (Apple Silicon and Intel, macOS 13+).
2. Open it and drag **Keychron Battery** into **Applications**.
3. Launch the app and choose **Enable Battery Access…** from its menu bar icon.
4. Enable **Keychron Battery** in **System Settings → Privacy & Security → Input Monitoring**. Restart the app if prompted.

No Rust, Xcode, or Homebrew installation is needed. A ZIP archive is also provided.

The app is locally signed, but not Apple-notarized. If macOS blocks it, attempt to open it, then use **System Settings → Privacy & Security → Open Anyway**.

When updating, macOS may require removing and re-adding **Keychron Battery** in
Input Monitoring because each ad-hoc-signed build has a different code identity.
Keep the app in Applications, then quit and reopen it after granting access.

### Included

- Native menu bar icon and percentage, with per-keyboard details.
- Standard HID battery reads refreshed every 30 seconds in the background.
- Manual refresh and diagnostics.
- Explicit unavailable and permission states.

Verified with a Bluetooth Keychron K2, firmware 1.0.9. Other models must expose a standard HID battery field; BLE-only battery reporting is not implemented in this version. Intel is cross-compiled; hardware testing was performed on Apple Silicon.

SHA-256 checksums are included in `SHA256SUMS.txt`.
