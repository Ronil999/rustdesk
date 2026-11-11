//! Blackout blanks the controlled machine's physical monitors while a remote session keeps
//! capturing and controlling the desktop.
//!
//! Only [`DisplayBackend`] implementations know how a platform blanks its outputs; the lease
//! bookkeeping here and the request handling in [`protocol`] never name a platform. A backend
//! for a new OS plugs in through `platform_backend` and `platform_is_supported`.

use std::{collections::HashSet, fmt, sync::Mutex};

pub mod protocol;

#[cfg(target_os = "linux")]
mod linux;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlackoutError {
    /// This host cannot blank its displays (OS, display server or driver).
    NotSupported(String),
    /// A supported backend tried and failed.
    Failed(String),
}

impl fmt::Display for BlackoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlackoutError::NotSupported(s) => write!(f, "not supported: {}", s),
            BlackoutError::Failed(s) => write!(f, "failed: {}", s),
        }
    }
}

pub trait DisplayBackend: Send {
    /// Blank every physical output. On error no output may be left blanked.
    fn blank(&mut self) -> Result<(), BlackoutError>;
    /// Return every output `blank` touched to the state it saved.
    fn restore(&mut self) -> Result<(), BlackoutError>;
}

pub type BackendFactory = fn() -> Result<Box<dyn DisplayBackend>, BlackoutError>;

/// Displays stay blanked while at least one session holds a lease, so one session ending
/// never restores a display another session still keeps dark.
pub struct Controller {
    factory: BackendFactory,
    active: Option<Box<dyn DisplayBackend>>,
    holders: HashSet<i32>,
}

impl Controller {
    pub fn new(factory: BackendFactory) -> Self {
        Self {
            factory,
            active: None,
            holders: HashSet::new(),
        }
    }

    pub fn acquire(&mut self, conn_id: i32) -> Result<(), BlackoutError> {
        if self.holders.contains(&conn_id) {
            return Ok(());
        }
        if self.active.is_none() {
            let mut backend = (self.factory)()?;
            backend.blank()?;
            self.active = Some(backend);
        }
        self.holders.insert(conn_id);
        Ok(())
    }

    pub fn release(&mut self, conn_id: i32) -> Result<(), BlackoutError> {
        if !self.holders.remove(&conn_id) || !self.holders.is_empty() {
            return Ok(());
        }
        match self.active.take() {
            Some(mut backend) => backend.restore(),
            None => Ok(()),
        }
    }

    pub fn is_held_by(&self, conn_id: i32) -> bool {
        self.holders.contains(&conn_id)
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

lazy_static::lazy_static! {
    static ref CONTROLLER: Mutex<Controller> = Mutex::new(Controller::new(platform_backend));
}

fn controller() -> std::sync::MutexGuard<'static, Controller> {
    // A panic elsewhere must not stop a later session from restoring the displays.
    CONTROLLER.lock().unwrap_or_else(|e| e.into_inner())
}

fn platform_backend() -> Result<Box<dyn DisplayBackend>, BlackoutError> {
    #[cfg(target_os = "linux")]
    return linux::new_backend();
    #[cfg(not(target_os = "linux"))]
    Err(BlackoutError::NotSupported(
        "Blackout is not available on this operating system yet".to_owned(),
    ))
}

/// Capability advertised to controllers in `PeerInfo.features.blackout`.
pub fn is_supported() -> bool {
    #[cfg(target_os = "linux")]
    return linux::is_supported();
    #[cfg(not(target_os = "linux"))]
    false
}

/// One session's hold on the blackout. Dropping it releases the hold.
pub struct Lease {
    conn_id: i32,
}

impl Lease {
    pub fn release(self) -> Result<(), BlackoutError> {
        let conn_id = self.conn_id;
        std::mem::forget(self);
        controller().release(conn_id)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Err(e) = controller().release(self.conn_id) {
            log::error!("blackout: release for #{} {}", self.conn_id, e);
        }
    }
}

pub fn acquire(conn_id: i32) -> Result<Lease, BlackoutError> {
    controller().acquire(conn_id)?;
    Ok(Lease { conn_id })
}

/// Handles a `ToggleBlackout` from session `conn_id` and returns the reply to send back.
/// The display work runs on a blocking thread.
pub async fn toggle(
    conn_id: i32,
    allowed: bool,
    on: bool,
    lease: &mut Option<Lease>,
) -> crate::message_proto::Message {
    let mut held = lease.take();
    let res = tokio::task::spawn_blocking(move || {
        let resp =
            protocol::handle_toggle(allowed, on, &mut held, || acquire(conn_id), Lease::release);
        (held, resp)
    })
    .await;
    let resp = match res {
        Ok((held, resp)) => {
            *lease = held;
            resp
        }
        Err(e) => protocol::response(
            crate::message_proto::blackout_response::Status::Failed,
            false,
            e.to_string(),
        ),
    };
    log::info!(
        "blackout: #{} on={} -> {:?} on={} {}",
        conn_id,
        on,
        resp.status.enum_value(),
        resp.on,
        resp.details
    );
    protocol::make_message(resp)
}

/// Releases a session's hold when the session ends.
pub async fn end_session(lease: Option<Lease>) {
    if let Some(lease) = lease {
        let conn_id = lease.conn_id;
        match tokio::task::spawn_blocking(move || lease.release()).await {
            Ok(Ok(())) => log::info!("blackout: #{} released on session end", conn_id),
            Ok(Err(e)) => log::error!("blackout: #{} release on session end {}", conn_id, e),
            Err(e) => log::error!("blackout: #{} release task {}", conn_id, e),
        }
    }
}

/// Runs the crash guard and exits when this process was started as one.
pub fn dispatch_guard() {
    #[cfg(target_os = "linux")]
    linux::recovery::dispatch_guard();
}

/// Undoes a blackout a crashed or killed server left behind.
pub fn recover_on_startup() {
    #[cfg(target_os = "linux")]
    linux::recovery::recover_on_startup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static BLANKS: AtomicUsize = AtomicUsize::new(0);
    static RESTORES: AtomicUsize = AtomicUsize::new(0);

    struct Counting;
    impl DisplayBackend for Counting {
        fn blank(&mut self) -> Result<(), BlackoutError> {
            BLANKS.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn restore(&mut self) -> Result<(), BlackoutError> {
            RESTORES.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn counting() -> Result<Box<dyn DisplayBackend>, BlackoutError> {
        Ok(Box::new(Counting))
    }

    fn unsupported() -> Result<Box<dyn DisplayBackend>, BlackoutError> {
        Err(BlackoutError::NotSupported("test".to_owned()))
    }

    struct FailingBlank;
    impl DisplayBackend for FailingBlank {
        fn blank(&mut self) -> Result<(), BlackoutError> {
            Err(BlackoutError::Failed("blank".to_owned()))
        }
        fn restore(&mut self) -> Result<(), BlackoutError> {
            Ok(())
        }
    }

    fn failing() -> Result<Box<dyn DisplayBackend>, BlackoutError> {
        Ok(Box::new(FailingBlank))
    }

    #[test]
    fn second_session_keeps_display_dark_until_last_release() {
        BLANKS.store(0, Ordering::SeqCst);
        RESTORES.store(0, Ordering::SeqCst);
        let mut c = Controller::new(counting);
        c.acquire(1).unwrap();
        c.acquire(1).unwrap();
        c.acquire(2).unwrap();
        assert_eq!(BLANKS.load(Ordering::SeqCst), 1);

        c.release(1).unwrap();
        assert!(c.is_active());
        assert_eq!(RESTORES.load(Ordering::SeqCst), 0);
        // a session that holds nothing cannot restore a display another session keeps dark
        c.release(3).unwrap();
        assert!(c.is_active());

        c.release(2).unwrap();
        c.release(2).unwrap();
        assert!(!c.is_active());
        assert_eq!(RESTORES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_or_unsupported_backend_grants_no_lease() {
        let mut c = Controller::new(unsupported);
        assert!(matches!(c.acquire(1), Err(BlackoutError::NotSupported(_))));
        assert!(!c.is_held_by(1));
        let mut c = Controller::new(failing);
        assert!(matches!(c.acquire(1), Err(BlackoutError::Failed(_))));
        assert!(!c.is_held_by(1) && !c.is_active());
    }
}
