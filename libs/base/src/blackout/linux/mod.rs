//! X11 backend: blanks each active CRTC by loading an all-zero gamma ramp.
//!
//! The gamma ramp is applied by the display controller at scanout, after the framebuffer, so the
//! root window RustDesk captures with MIT-SHM keeps its real pixels while the monitor shows black.
//! DPMS was rejected because every injected input event wakes the monitor again, and turning an
//! output off changes the screen layout the capturer works from.

use super::{BlackoutError, DisplayBackend};
use serde_derive::{Deserialize, Serialize};
use std::{path::PathBuf, process::Child};

pub(super) mod recovery;
mod xcb;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ramp {
    pub red: Vec<u16>,
    pub green: Vec<u16>,
    pub blue: Vec<u16>,
}

impl Ramp {
    pub fn black(size: usize) -> Self {
        Self {
            red: vec![0; size],
            green: vec![0; size],
            blue: vec![0; size],
        }
    }

    pub fn len(&self) -> usize {
        self.red.len()
    }

    pub fn is_black(&self) -> bool {
        self.len() > 0
            && [&self.red, &self.green, &self.blue]
                .iter()
                .all(|c| c.iter().all(|v| *v == 0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crtc {
    pub id: u32,
    pub outputs: Vec<String>,
}

/// The RandR gamma calls the backend needs; a mock stands in for the X server in tests.
pub trait GammaApi {
    /// CRTCs driving a connected output.
    fn active_crtcs(&mut self) -> Result<Vec<Crtc>, BlackoutError>;
    fn gamma(&mut self, crtc: u32) -> Result<Ramp, BlackoutError>;
    fn set_gamma(&mut self, crtc: u32, ramp: &Ramp) -> Result<(), BlackoutError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedCrtc {
    pub crtc: u32,
    pub outputs: Vec<String>,
    pub ramp: Ramp,
}

/// Rejects sessions whose monitors X11 gamma cannot reach.
pub fn check_session(display_server: &str, display: Option<&str>) -> Result<(), BlackoutError> {
    if display_server == crate::platform::linux::DISPLAY_SERVER_WAYLAND {
        return Err(BlackoutError::NotSupported(
            "Wayland sessions are not supported; log in with an Xorg session".to_owned(),
        ));
    }
    if display.map_or(true, str::is_empty) {
        return Err(BlackoutError::NotSupported(
            "No X11 display in this session".to_owned(),
        ));
    }
    Ok(())
}

fn current_session() -> Result<(), BlackoutError> {
    check_session(
        &crate::platform::linux::get_display_server(),
        std::env::var("DISPLAY").ok().as_deref(),
    )
}

fn names(crtcs: &[SavedCrtc]) -> String {
    crtcs
        .iter()
        .flat_map(|c| c.outputs.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Saves every active CRTC's ramp, hands the saved state to `persist` before touching anything,
/// then blanks them. A CRTC that fails rolls back the ones already blanked.
pub fn blank_all<G: GammaApi>(
    api: &mut G,
    persist: impl FnOnce(&[SavedCrtc]) -> Result<(), BlackoutError>,
) -> Result<Vec<SavedCrtc>, BlackoutError> {
    let crtcs = api.active_crtcs()?;
    if crtcs.is_empty() {
        return Err(BlackoutError::Failed(
            "No connected monitor was found".to_owned(),
        ));
    }
    let mut saved = Vec::with_capacity(crtcs.len());
    for c in crtcs {
        let ramp = api.gamma(c.id)?;
        if ramp.len() == 0 {
            return Err(BlackoutError::NotSupported(format!(
                "The driver of {} has no gamma ramp",
                c.outputs.join(", ")
            )));
        }
        saved.push(SavedCrtc {
            crtc: c.id,
            outputs: c.outputs,
            ramp,
        });
    }
    persist(&saved)?;
    for (i, c) in saved.iter().enumerate() {
        if let Err(e) = api.set_gamma(c.crtc, &Ramp::black(c.ramp.len())) {
            if let Err(rollback) = restore_all(api, &saved[..i]) {
                log::error!("blackout: rollback {}", rollback);
            }
            return Err(BlackoutError::Failed(format!(
                "Could not blank {}: {}",
                c.outputs.join(", "),
                e
            )));
        }
    }
    Ok(saved)
}

/// Puts every saved ramp back, carrying on past a CRTC that fails.
pub fn restore_all<G: GammaApi>(api: &mut G, saved: &[SavedCrtc]) -> Result<(), BlackoutError> {
    let failed: Vec<SavedCrtc> = saved
        .iter()
        .filter(|c| match api.set_gamma(c.crtc, &c.ramp) {
            Ok(()) => false,
            Err(e) => {
                log::error!("blackout: restore {}: {}", c.outputs.join(", "), e);
                true
            }
        })
        .cloned()
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(BlackoutError::Failed(format!(
            "Could not restore {}",
            names(&failed)
        )))
    }
}

/// After a crash, restores only CRTCs still black with the saved ramp size: a ramp someone
/// changed since, or one a restarted X server reset, is left alone.
pub fn recover<G: GammaApi>(api: &mut G, saved: &[SavedCrtc]) -> usize {
    saved
        .iter()
        .filter(|c| match api.gamma(c.crtc) {
            Ok(cur) if cur.is_black() && cur.len() == c.ramp.len() => {
                api.set_gamma(c.crtc, &c.ramp).is_ok()
            }
            _ => false,
        })
        .count()
}

struct X11Backend {
    saved: Vec<SavedCrtc>,
    state_path: PathBuf,
    guard: Option<Child>,
}

impl DisplayBackend for X11Backend {
    fn blank(&mut self) -> Result<(), BlackoutError> {
        current_session()?;
        let mut api = xcb::XcbGamma::connect()?;
        let path = self.state_path.clone();
        self.saved = blank_all(&mut api, |s| recovery::save(&path, s))?;
        match recovery::spawn_guard() {
            Ok(child) => self.guard = Some(child),
            // Startup recovery still covers a crash; the session goes on without the guard.
            Err(e) => log::warn!("blackout: guard not started: {}", e),
        }
        log::info!("blackout: blanked {}", names(&self.saved));
        Ok(())
    }

    fn restore(&mut self) -> Result<(), BlackoutError> {
        let res = xcb::XcbGamma::connect().and_then(|mut api| restore_all(&mut api, &self.saved));
        if res.is_ok() {
            recovery::remove(&self.state_path);
            log::info!("blackout: restored {}", names(&self.saved));
        }
        // The guard retries from the state file if restoring failed here.
        recovery::stop_guard(self.guard.take());
        res
    }
}

pub fn new_backend() -> Result<Box<dyn DisplayBackend>, BlackoutError> {
    current_session()?;
    Ok(Box::new(X11Backend {
        saved: Vec::new(),
        state_path: recovery::state_path(),
        guard: None,
    }))
}

pub fn is_supported() -> bool {
    let probe = current_session().and_then(|_| {
        let mut api = xcb::XcbGamma::connect()?;
        let crtcs = api.active_crtcs()?;
        let first = crtcs
            .first()
            .ok_or_else(|| BlackoutError::Failed("no monitor".to_owned()))?;
        Ok(api.gamma(first.id)?.len() > 0)
    });
    match probe {
        Ok(supported) => supported,
        Err(e) => {
            log::info!("blackout: {}", e);
            false
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashSet};

    #[derive(Default)]
    pub(crate) struct MockX {
        pub crtcs: Vec<Crtc>,
        pub ramps: BTreeMap<u32, Ramp>,
        pub fail_set: HashSet<u32>,
        pub no_randr: bool,
    }

    impl MockX {
        pub(crate) fn with_monitors(sizes: &[(u32, &str, usize)]) -> Self {
            let mut m = MockX::default();
            for (id, name, size) in sizes {
                m.crtcs.push(Crtc {
                    id: *id,
                    outputs: vec![name.to_string()],
                });
                let ramp: Vec<u16> = (0..*size).map(|i| (i * 257) as u16).collect();
                m.ramps.insert(
                    *id,
                    Ramp {
                        red: ramp.clone(),
                        green: ramp.iter().map(|v| v / 2).collect(),
                        blue: ramp,
                    },
                );
            }
            m
        }
    }

    impl GammaApi for MockX {
        fn active_crtcs(&mut self) -> Result<Vec<Crtc>, BlackoutError> {
            if self.no_randr {
                return Err(BlackoutError::NotSupported("RandR 1.3 missing".to_owned()));
            }
            Ok(self.crtcs.clone())
        }
        fn gamma(&mut self, crtc: u32) -> Result<Ramp, BlackoutError> {
            self.ramps
                .get(&crtc)
                .cloned()
                .ok_or_else(|| BlackoutError::Failed(format!("BadRRCrtc {}", crtc)))
        }
        fn set_gamma(&mut self, crtc: u32, ramp: &Ramp) -> Result<(), BlackoutError> {
            if self.fail_set.contains(&crtc) || !self.ramps.contains_key(&crtc) {
                return Err(BlackoutError::Failed(format!("BadRRCrtc {}", crtc)));
            }
            self.ramps.insert(crtc, ramp.clone());
            Ok(())
        }
    }

    #[test]
    fn x11_and_wayland_detection() {
        assert!(check_session("x11", Some(":0")).is_ok());
        assert!(matches!(
            check_session("wayland", Some(":0")),
            Err(BlackoutError::NotSupported(_))
        ));
        assert!(matches!(
            check_session("x11", None),
            Err(BlackoutError::NotSupported(_))
        ));
        assert!(matches!(
            check_session("x11", Some("")),
            Err(BlackoutError::NotSupported(_))
        ));
    }

    #[test]
    fn blanks_every_monitor_and_restores_exact_ramps() {
        let mut x = MockX::with_monitors(&[(63, "DP-1", 256), (64, "HDMI-1", 1024)]);
        let original = x.ramps.clone();
        let mut persisted = None;
        let saved = blank_all(&mut x, |s| {
            persisted = Some(s.to_vec());
            Ok(())
        })
        .unwrap();
        assert!(x.ramps.values().all(Ramp::is_black));
        assert_eq!(persisted.as_ref(), Some(&saved));
        restore_all(&mut x, &saved).unwrap();
        assert_eq!(x.ramps, original);
    }

    #[test]
    fn partial_failure_rolls_back_and_reports_the_output() {
        let mut x = MockX::with_monitors(&[(63, "DP-1", 256), (64, "HDMI-1", 256)]);
        let original = x.ramps.clone();
        x.fail_set.insert(64);
        let err = blank_all(&mut x, |_| Ok(())).unwrap_err();
        assert!(matches!(&err, BlackoutError::Failed(s) if s.contains("HDMI-1")));
        assert_eq!(x.ramps, original);
    }

    #[test]
    fn nothing_changes_when_state_cannot_be_saved_or_randr_is_missing() {
        let mut x = MockX::with_monitors(&[(63, "DP-1", 256)]);
        let original = x.ramps.clone();
        assert!(blank_all(&mut x, |_| Err(BlackoutError::Failed("disk".to_owned()))).is_err());
        assert_eq!(x.ramps, original);
        x.no_randr = true;
        assert!(matches!(
            blank_all(&mut x, |_| Ok(())),
            Err(BlackoutError::NotSupported(_))
        ));
    }

    #[test]
    fn invalid_output_and_gammaless_driver() {
        let mut x = MockX::with_monitors(&[(63, "DP-1", 0)]);
        assert!(matches!(
            blank_all(&mut x, |_| Ok(())),
            Err(BlackoutError::NotSupported(_))
        ));
        let mut x = MockX::default();
        assert!(matches!(
            blank_all(&mut x, |_| Ok(())),
            Err(BlackoutError::Failed(_))
        ));
        // a monitor unplugged while dark: the others are still restored, the error names it
        let mut x = MockX::with_monitors(&[(63, "DP-1", 256), (64, "HDMI-1", 256)]);
        let saved = blank_all(&mut x, |_| Ok(())).unwrap();
        x.ramps.remove(&64);
        let err = restore_all(&mut x, &saved).unwrap_err();
        assert!(matches!(&err, BlackoutError::Failed(s) if s.contains("HDMI-1")));
        assert!(!x.ramps[&63].is_black());
    }

    #[test]
    fn recovery_restores_only_ramps_still_black() {
        let mut x = MockX::with_monitors(&[(63, "DP-1", 256), (64, "HDMI-1", 256)]);
        let original = x.ramps.clone();
        let saved = blank_all(&mut x, |_| Ok(())).unwrap();
        // HDMI-1's ramp was changed since (X restart, night light); DP-1 is still dark
        let mut tweaked = original[&64].clone();
        tweaked.red[1] = 1;
        x.ramps.insert(64, tweaked.clone());
        assert_eq!(recover(&mut x, &saved), 1);
        assert_eq!(x.ramps[&63], original[&63]);
        assert_eq!(x.ramps[&64], tweaked);
        // running it twice is harmless
        assert_eq!(recover(&mut x, &saved), 0);
    }
}
