//! Keeps a blackout from outliving the process that started it.
//!
//! Before blanking, the original ramps go to a state file. A guard child process blocks on a
//! pipe from the server; the pipe closes however the server ends (normal exit, crash, SIGKILL),
//! and the guard then restores from the file. The server also restores from a leftover file
//! when it starts. Both only touch CRTCs that are still black, so running either twice, or
//! after a reboot already reset the gamma, changes nothing.

use super::{recover, xcb::XcbGamma, BlackoutError, GammaApi, SavedCrtc};
use serde_derive::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub const GUARD_ARG: &str = "--blackout-guard";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedState {
    /// The X display the ramps belong to.
    pub display: String,
    pub crtcs: Vec<SavedCrtc>,
}

pub fn state_path() -> PathBuf {
    hbb_common::config::Config::path("blackout_state.json")
}

fn current_display() -> String {
    std::env::var("DISPLAY").unwrap_or_default()
}

pub fn save(path: &Path, crtcs: &[SavedCrtc]) -> Result<(), BlackoutError> {
    save_state(
        path,
        &SavedState {
            display: current_display(),
            crtcs: crtcs.to_vec(),
        },
    )
    .map_err(|e| BlackoutError::Failed(format!("Cannot save the display state: {}", e)))
}

fn save_state(path: &Path, state: &SavedState) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec(state).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(&json)?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

pub fn load(path: &Path) -> Option<SavedState> {
    let data = fs::read(path).ok()?;
    match serde_json::from_slice(&data) {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!("blackout: discarding unreadable state file: {}", e);
            remove(path);
            None
        }
    }
}

pub fn remove(path: &Path) {
    if let Err(e) = fs::remove_file(path) {
        if e.kind() != io::ErrorKind::NotFound {
            log::warn!("blackout: remove state file: {}", e);
        }
    }
}

/// Restores a leftover state file. Returns how many CRTCs were restored, or `None` when the
/// X server could not be reached, in which case the file is kept for a later attempt.
pub fn recover_file<G: GammaApi>(
    path: &Path,
    display: &str,
    connect: impl FnOnce() -> Result<G, BlackoutError>,
) -> Option<usize> {
    let state = load(path)?;
    if state.display != display {
        // Another X server; the one these ramps belonged to has reset them on exit.
        remove(path);
        return Some(0);
    }
    let mut api = match connect() {
        Ok(api) => api,
        Err(e) => {
            log::warn!("blackout: recovery postponed: {}", e);
            return None;
        }
    };
    let n = recover(&mut api, &state.crtcs);
    remove(path);
    Some(n)
}

pub fn recover_on_startup() {
    if let Some(n) = recover_file(&state_path(), &current_display(), XcbGamma::connect) {
        if n > 0 {
            log::info!(
                "blackout: restored {} display(s) left dark by a previous run",
                n
            );
        }
    }
}

pub fn spawn_guard() -> io::Result<Child> {
    Command::new(std::env::current_exe()?)
        .arg(GUARD_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

pub fn stop_guard(child: Option<Child>) {
    if let Some(mut child) = child {
        // EOF tells the guard to run; it finds the ramps already restored and exits.
        drop(child.stdin.take());
        if let Err(e) = child.wait() {
            log::warn!("blackout: wait guard: {}", e);
        }
    }
}

pub fn dispatch_guard() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(GUARD_ARG)) || args.next().is_some() {
        return;
    }
    // A service stop signals the whole process group; outlive the server so its exit closes
    // the pipe and the restore still runs.
    unsafe {
        for sig in [
            hbb_common::libc::SIGTERM,
            hbb_common::libc::SIGINT,
            hbb_common::libc::SIGHUP,
        ] {
            hbb_common::libc::signal(sig, hbb_common::libc::SIG_IGN);
        }
    }
    io::copy(&mut io::stdin(), &mut io::sink()).ok();
    recover_on_startup();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::super::{blank_all, tests::MockX, Ramp};
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rustdesk-blackout-{}-{}", name, std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("blackout_state.json")
    }

    #[test]
    fn recovery_after_restart_restores_saved_ramps_and_clears_the_file() {
        let path = temp_path("restart");
        let mut x = MockX::with_monitors(&[(63, "DP-1", 256), (64, "HDMI-1", 256)]);
        let original = x.ramps.clone();
        let saved = blank_all(&mut x, |s| {
            save_state(
                &path,
                &SavedState {
                    display: ":1".to_owned(),
                    crtcs: s.to_vec(),
                },
            )
            .map_err(|e| BlackoutError::Failed(e.to_string()))
        })
        .unwrap();
        assert_eq!(saved.len(), 2);
        assert!(x.ramps.values().all(Ramp::is_black));
        // the server process is gone here; a new one starts with the same X display
        assert_eq!(recover_file(&path, ":1", || Ok(&mut x)), Some(2));
        assert_eq!(x.ramps, original);
        assert!(!path.exists());
        assert_eq!(recover_file(&path, ":1", || Ok(&mut x)), None);
    }

    #[test]
    fn recovery_keeps_file_when_x_is_unreachable_and_drops_it_for_another_display() {
        let path = temp_path("unreachable");
        let state = SavedState {
            display: ":0".to_owned(),
            crtcs: vec![],
        };
        save_state(&path, &state).unwrap();
        assert_eq!(load(&path), Some(state));
        let none: Result<MockX, _> = Err(BlackoutError::NotSupported("no X".to_owned()));
        assert_eq!(recover_file(&path, ":0", || none), None);
        assert!(path.exists());
        assert_eq!(
            recover_file::<MockX>(&path, ":1", || panic!("connected")),
            Some(0)
        );
        assert!(!path.exists());
    }

    #[test]
    fn corrupt_state_file_is_discarded() {
        let path = temp_path("corrupt");
        fs::write(&path, b"{not json").unwrap();
        assert_eq!(
            recover_file::<MockX>(&path, ":0", || panic!("connected")),
            None
        );
        assert!(!path.exists());
    }

    impl GammaApi for &mut MockX {
        fn active_crtcs(&mut self) -> Result<Vec<super::super::Crtc>, BlackoutError> {
            (**self).active_crtcs()
        }
        fn gamma(&mut self, crtc: u32) -> Result<Ramp, BlackoutError> {
            (**self).gamma(crtc)
        }
        fn set_gamma(&mut self, crtc: u32, ramp: &Ramp) -> Result<(), BlackoutError> {
            (**self).set_gamma(crtc, ramp)
        }
    }
}
