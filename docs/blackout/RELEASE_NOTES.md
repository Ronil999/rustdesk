## RustDesk Blackout fork

**Unofficial build.** This is an independently maintained fork of RustDesk 1.5.3 with one added
feature, *Blackout*. It is not made or supported by the RustDesk team. Source code (AGPL-3.0):
this repository, at the commit named below.

### New: Blackout

Remote toolbar → **Control Actions** → **Blackout** turns the controlled machine's physical
monitors black while your view of its desktop, the mouse and the keyboard keep working. Turning it
off, disconnecting, or the session ending for any reason (including a crash) restores the monitors.

### Downloads

| Architecture | Windows | Ubuntu | Checksums |
|---|---|---|---|
| x86-64 (64-bit) | [EXE](https://github.com/Ronil999/rustdesk/releases/download/{TAG}/RustDeskBlackout-1.5.3-windows-x86_64.exe) &nbsp; [ZIP](https://github.com/Ronil999/rustdesk/releases/download/{TAG}/RustDeskBlackout-1.5.3-windows-x86_64.zip) | [Download (.deb)](https://github.com/Ronil999/rustdesk/releases/download/{TAG}/rustdesk-blackout-1.5.3-ubuntu-x86_64.deb) | [SHA256SUMS](https://github.com/Ronil999/rustdesk/releases/download/{TAG}/SHA256SUMS.txt) |

macOS, Android, iOS, Flatpak, Web, ARM and 32-bit builds are not available for this fork.

- **Windows 10/11:** EXE runs or installs the app; ZIP is the same app as a plain folder.
- **Ubuntu 18.04+:** `sudo apt install ./rustdesk-blackout-1.5.3-ubuntu-x86_64.deb`, then run `rustdeskblackout`.
- **Verify:** `sha256sum -c SHA256SUMS.txt --ignore-missing`

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
