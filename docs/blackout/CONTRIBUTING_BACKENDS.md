# Adding a Blackout display backend

The protocol, the toolbar, authorization and session leases are OS-independent. A new
controlled-side platform needs only a backend. Nothing in `message.proto`, `connection.rs` or
the Flutter code changes.

## 1. Implement `DisplayBackend`

```rust
// libs/base/src/blackout/mod.rs
pub trait DisplayBackend: Send {
    /// Blank every physical output. On error no output may be left blanked.
    fn blank(&mut self) -> Result<(), BlackoutError>;
    /// Return every output `blank` touched to the state it saved.
    fn restore(&mut self) -> Result<(), BlackoutError>;
}
```

Put it in its own module, for example `libs/base/src/blackout/windows/mod.rs`, behind
`#[cfg(target_os = "windows")]`. The `Controller` creates one backend when the first session
asks for Blackout, calls `blank` once, and calls `restore` once when the last session lets go.
The backend does not need to count sessions.

Return `BlackoutError::NotSupported` for "this machine cannot do it", which the UI shows as not
supported, and `BlackoutError::Failed` for "it should work but did not".

Rules every backend must keep:

- **The captured image must not change.** Blank the scanout (gamma/colour LUT, panel power), not
  the framebuffer or composited desktop the capturer reads. Never draw a black window, because
  the capturer sees it. Verify with a real capture while blanked, like
  `blank_keeps_capture` in `linux/xcb.rs`.
- **Input must not undo it.** The remote session injects input. Anything that an input event
  wakes (DPMS, monitor sleep) does not work.
- **No layout or resolution changes.** Turning an output off moves the screens the capturer
  works from.
- **Save before you change, and roll back on partial failure** (see `blank_all`).
- **Survive a crash.** Persist the saved state before blanking, restore it on startup, and
  restore when the server process dies (see `linux/recovery.rs`). Only restore outputs that are
  still in the blanked state.
- **Never build a command line from network input.** The request carries only a bool.

## 2. Register it

In `libs/base/src/blackout/mod.rs`:

```rust
#[cfg(target_os = "windows")]
mod windows;

fn platform_backend() -> Result<Box<dyn DisplayBackend>, BlackoutError> {
    #[cfg(target_os = "windows")]
    return windows::new_backend();
    // ...
}

pub fn is_supported() -> bool {
    #[cfg(target_os = "windows")]
    return windows::is_supported();
    // ...
}
```

`is_supported` is sent to every controller in `PeerInfo.features.blackout`. Make it a cheap probe
that does not change anything. If the backend needs crash recovery, hook it into
`dispatch_guard` / `recover_on_startup` there too.

On Windows the server runs as a service in session 0, while the monitors belong to the user's
session. A Windows backend must do its display work in the user session, for example through
the existing session helpers in `src/platform/windows.rs`.

## 3. Test it

- Put the logic behind a small trait over the OS calls, as `GammaApi` does on Linux, so unit
  tests can run against a mock: multiple displays, partial failure, invalid output, recovery.
- Add an `#[ignore]` hardware test that blanks for about a second and checks the capture.
- CI: Windows tests run in `blackout-build.yml` (`cargo test -p base`). A macOS backend needs a
  macOS job there.

## 4. Update the docs

Add the platform to the *Supported combinations* table in `README_BLACKOUT.md` and to
`docs/blackout/RELEASE_NOTES.md`.
