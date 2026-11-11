# RustDesk Blackout fork

> **This is an independently maintained fork of [RustDesk](https://github.com/rustdesk/rustdesk),
> not an official RustDesk release.** It adds one feature, *Blackout*. RustDesk is licensed under
> AGPL-3.0 (see [LICENCE](LICENCE)); the full source of every build of this fork is in this
> repository. "RustDesk" and its logo are trademarks of their owners and are used here only to
> name the upstream project.

## What Blackout does

While you control a remote machine, **Blackout** makes that machine's physical monitors black.
Your own window keeps showing the remote desktop normally. Mouse, keyboard, and the remote
machine's running applications are not affected.

- **ON**: the remote monitors turn black and your view is unchanged.
- **OFF**: the remote monitors return to exactly how they were before.
- **When you disconnect** (or the network drops, or RustDesk quits or crashes), the remote
  monitors are restored automatically.

Blackout never turns on by itself. It only turns on when an authorized controller clicks it.
It is separate from RustDesk's *Privacy mode* and does not use it.

## Where to find it

Open a remote session, then go to the floating toolbar → **Control Actions** (the lightning-bolt
icon) → **Blackout**. The entry shows the current state:

| Shown        | Meaning                                       |
|--------------|-----------------------------------------------|
| `Off` (grey) | the remote monitors are normal                |
| spinner      | the request is being carried out              |
| `On` (green) | the remote monitors are black                 |

A short message confirms each change. If the remote machine cannot do Blackout, you get a
message saying why, and the entry never shows `On`.

## Supported combinations

| Controller (you)    | Controlled machine            | Status                   |
|---------------------|-------------------------------|--------------------------|
| Windows 10/11 x64   | Ubuntu, **Xorg (X11)** session | supported                |
| Ubuntu x64          | Ubuntu, **Xorg (X11)** session | supported                |
| any                 | Ubuntu, Wayland session       | reports "not supported"  |
| any                 | Windows / macOS               | not yet (reports "not supported") |
| official RustDesk   | this fork                     | no Blackout button       |
| this fork           | official RustDesk             | reports "not supported"  |

Both computers need this fork for Blackout. With other versions, normal RustDesk features
still work.

### Ubuntu requirements (controlled machine)

- You must be logged into an **Xorg** session. On Ubuntu's login screen, click the gear icon
  and choose **"Ubuntu on Xorg"**. To check, run `echo $XDG_SESSION_TYPE`, which must print `x11`.
- No sudo or extra packages are needed at runtime. The required libraries ship with the
  Ubuntu desktop.

## Installing

Downloads are on this repository's **Releases** page (or, for test builds, the artifacts of the
*Blackout fork build* workflow run). Check the download against `SHA256SUMS.txt`:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```

### Ubuntu

```bash
sudo apt install ./rustdesk-1.5.3-blackout-ubuntu-x86_64.deb
```

The package is named `rustdesk`, so it **replaces an installed official RustDesk**. Your
RustDesk ID, password and settings are kept. To go back, install the official `.deb` the same
way.

After installing, log out and back in (or reboot) once so the RustDesk service starts in your
Xorg session.

### Windows

- `rustdesk-1.5.3-blackout-windows-x86_64.exe`: run it. It works without installing, and the
  app offers *Install* from its main window.
- `rustdesk-1.5.3-blackout-windows-x86_64.zip`: the same files as a plain folder.

The Windows build is **not code-signed**, so Windows SmartScreen will warn. Click
*More info → Run anyway*.

> **Do not accept RustDesk's in-app "update available" prompt.** It installs the official
> build, which has no Blackout.

## How it works

### Architecture

```
Flutter toolbar (shared by Windows and Linux controllers)
  flutter/lib/common/widgets/blackout_menu.dart   menu entry, toasts
  flutter/lib/models/blackout_model.dart          per-session state machine
        │  bind.sessionToggleBlackout(on)
Rust controller side
  src/flutter_ffi.rs → src/ui_session_interface.rs → network
        │  Misc.toggle_blackout  /  Misc.blackout_response
Rust controlled side
  src/server/connection.rs           authorization, per-session lease, release on close
  libs/base/src/blackout/mod.rs      Controller + DisplayBackend trait (OS-independent)
  libs/base/src/blackout/protocol.rs request handling (OS-independent, unit tested)
  libs/base/src/blackout/linux/      X11 backend, crash recovery
```

### Protocol

Defined in `libs/base/protos/message.proto` and carried in the existing authenticated,
encrypted session:

| Message                           | Direction          | Content                                    |
|-----------------------------------|--------------------|--------------------------------------------|
| `PeerInfo.features.blackout`      | host → controller  | `true` when this host can do Blackout now  |
| `Misc.toggle_blackout`            | controller → host  | `ToggleBlackout { bool on }`               |
| `Misc.blackout_response`          | host → controller  | `status` (Ok / NotSupported / Denied / Failed), `on`, `details` |

- Field numbers start at 1001 so they cannot collide with fields upstream RustDesk adds later.
- **Older hosts** do not send the feature, so the controller treats Blackout as unsupported and
  never sends the request.
- **Older controllers** never send the request.
- The host answers every request with the state it actually holds. Repeating ON or OFF is a
  no-op that answers `Ok`.

### Authorization and sessions

The host accepts a request only from an authenticated **remote-control** session that has the
**keyboard/mouse (control) permission**. File transfer, view-camera, terminal and port-forward
sessions are refused, as is a session the host restricted to view-only.

Each session holds its own *lease*. The monitors stay black while at least one session holds a
lease, so one session ending never restores monitors another session still keeps dark. Closing a
session releases its lease.

### Linux backend: why gamma, not DPMS

The backend sets every active CRTC's **gamma ramp** to zero, through native X RandR
(`xcb-randr`), and restores the exact saved ramp afterwards. The alternatives were rejected:

| Method                           | Monitor          | Remote image                    | Verdict  |
|----------------------------------|------------------|---------------------------------|----------|
| CRTC gamma ramp = 0 (chosen)     | black            | unchanged (gamma is applied at scanout, after the framebuffer RustDesk captures) | ✅ |
| `xrandr --brightness 0`          | black            | unchanged (same mechanism)      | ✅, but only restores an approximation, and needs the `xrandr` tool |
| DPMS off (`xset dpms force off`) | off              | unchanged                       | ❌ every injected remote mouse/key event wakes the monitor |
| `xrandr --output … --off`        | off              | **display removed**             | ❌ changes the layout, and capture loses the screen |
| black fullscreen window          | black            | **black too**                   | ❌ it is captured |

On the development machine, the captured root image was measured with all ramps at zero and was
identical to the image before blanking (see *Testing* below).

Native RandR also means no `DISPLAY`, `XAUTHORITY` or user ID is hardcoded. The backend uses the
X session the RustDesk server already runs in, and nothing from the network reaches a command
line.

### Recovery

1. **Before** blanking, the original ramps are written to
   `~/.config/rustdesk/blackout_state.json` (mode 0600).
2. A small **guard** process (`rustdesk --blackout-guard`) waits on a pipe from the server. It
   is event-driven and does not poll. The pipe closes however the server ends, including a
   crash or `SIGKILL`, and the guard then restores the ramps. It ignores SIGTERM/SIGHUP, so a
   service stop that signals the whole group still triggers the restore.
3. When the server starts, it also restores from a leftover state file.

Both recovery paths touch only monitors that are **still black** with the saved ramp size. A
ramp changed since, or reset by a reboot or X restart, is left alone.

## Known limitations

- **Wayland is not supported.** Wayland compositors do not let an X client set gamma on real
  outputs. The controller gets a clear "not supported" message.
- **Night Light / colour profiles:** GNOME Night Light or a colour-management daemon may rewrite
  the gamma ramp while Blackout is on, which makes the monitor visible again. Turn Night Light off
  on the controlled machine.
- **Hotplug:** a monitor plugged in while Blackout is on is not blanked. Toggle Blackout off and
  on again.
- **Hardware cursor:** on some GPU drivers the hardware cursor plane bypasses the gamma ramp, so
  the mouse pointer can stay visible on the black screen.
- **LCD backlight:** a black LCD panel still has its backlight on. Gamma blanking hides the
  content and does not switch the panel off.
- **The person at the controlled machine cannot see anything while Blackout is on**, including
  the RustDesk connection window. To get the screen back without the controller, they can
  switch to a text console and back (`Ctrl+Alt+F3`, then `Ctrl+Alt+F2`/`F1`), unplug the network,
  or reboot.
- **Permission revoked mid-session:** if the host user revokes the control permission during a
  session, Blackout stays on until that session ends.
- **Power loss, a GPU reset, or the monitor's own settings**: if the X server or the display
  hardware goes away, there is nothing to restore. The ramps reset when X restarts.
- **Web client** (`flutter/lib/web`): no Blackout.

## Troubleshooting

| Symptom | Fix |
|---|---|
| "does not support Blackout" | Both sides need this fork. On Ubuntu, log in with "Ubuntu on Xorg". |
| "refused Blackout" | The host must allow keyboard/mouse control for your session. Turn view-only off. |
| Monitor stays black after the session | Log out and back in, or run `xrandr --output <name> --brightness 1` in the session. Please report it, with `~/.local/share/logs/RustDesk/server/*.log` (lines containing `blackout:`). |
| Pointer still visible | Hardware-cursor limitation of the driver (see above). |
| Comes back after a while | Night Light or a colour daemon reset the gamma. Disable it. |

## Building

### Locally on Ubuntu (Linux)

Follow the [official build guide](https://rustdesk.com/docs/en/dev/build/linux/). The packages
the full build needs (requires sudo):

```bash
sudo apt install -y g++ gcc git curl nasm yasm clang libclang-dev cmake make ninja-build \
  pkg-config libgtk-3-dev libxcb-randr0-dev libxdo-dev libxfixes-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libasound2-dev libpulse-dev libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev libva-dev libpam0g-dev
```

The Blackout core tests need only `libxcb-randr0-dev` and Rust:

```bash
cargo test -p base blackout                        # mocked X server, runs anywhere
cargo test -p base -- --ignored xcb_gamma_round_trip  # real X11, no visible change
cargo test -p base -- --ignored blank_keeps_capture   # real X11, blanks ~1.5 s
cd flutter && flutter test test/blackout_test.dart
```

### CI (Windows and Ubuntu)

`.github/workflows/blackout-build.yml` builds Windows x64 (native `windows-2022` runner) and
Ubuntu x64 (Ubuntu 18.04 container, like the official deb) from the same commit, and runs the
Rust and Dart tests:

- **Push to `feature/blackout-support`**: packages are uploaded as workflow artifacts.
- **Push a tag such as `v1.0.0-blackout`**: as above, plus a **draft** GitHub release with both
  packages, `SHA256SUMS.txt` and the release notes. Review the draft, then press *Publish*.

```bash
git tag v1.0.0-blackout && git push origin v1.0.0-blackout
```

## Adding Windows or macOS support

See [docs/blackout/CONTRIBUTING_BACKENDS.md](docs/blackout/CONTRIBUTING_BACKENDS.md).
