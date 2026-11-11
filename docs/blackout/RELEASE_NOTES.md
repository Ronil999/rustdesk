## RustDesk Blackout fork

**Unofficial build.** This is an independently maintained fork of RustDesk 1.5.3 with one added
feature, *Blackout*. It is not made or supported by the RustDesk team. Source code (AGPL-3.0):
this repository, at the commit named below.

### New: Blackout

Remote toolbar → **Control Actions** → **Blackout** turns the controlled machine's physical
monitors black while your view of its desktop, the mouse and the keyboard keep working. Turning it
off, disconnecting, or the session ending for any reason (including a crash) restores the monitors.

### Downloads

| File | For |
|---|---|
| `RustDeskBlackout-1.5.3-windows-x86_64.exe` | Windows 10/11 x64, run or install |
| `RustDeskBlackout-1.5.3-windows-x86_64.zip` | Windows 10/11 x64, plain folder |
| `rustdesk-blackout-1.5.3-ubuntu-x86_64.deb` | Ubuntu 18.04+ x64: `sudo apt install ./<file>`, then run `rustdeskblackout` |
| `SHA256SUMS.txt` | `sha256sum -c SHA256SUMS.txt --ignore-missing` |

### Installs beside official RustDesk

The app is called **RustDesk Blackout** and has its own install folder, service, settings, ID and
password, so an official RustDesk on the same computer keeps working untouched. Connect using the
**fork's ID** (shown in its main window).

### Supported

- Controllers: Windows x64 and Ubuntu x64.
- Controlled machine: Ubuntu with an **Xorg (X11)** session ("Ubuntu on Xorg" at login).
- Both computers must run this fork.

### Known limitations

- Wayland sessions, and Windows or macOS as the controlled machine, report "not supported".
- GNOME Night Light can undo the blackout. Disable it on the controlled machine.
- On some GPU drivers the mouse pointer stays visible, and LCD backlights stay lit.
- The person at the controlled machine cannot see the screen while Blackout is on.
- The Windows build is not code-signed (SmartScreen warning).

Full details: `README_BLACKOUT.md`.
