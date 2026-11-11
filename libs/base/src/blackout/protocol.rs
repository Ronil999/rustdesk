use super::BlackoutError;
use crate::message_proto::{blackout_response::Status, BlackoutResponse, Message, Misc};

pub fn response(status: Status, on: bool, details: String) -> BlackoutResponse {
    BlackoutResponse {
        status: status.into(),
        on,
        details,
        ..Default::default()
    }
}

pub fn make_message(resp: BlackoutResponse) -> Message {
    let mut misc = Misc::new();
    misc.set_blackout_response(resp);
    let mut msg = Message::new();
    msg.set_misc(misc);
    msg
}

pub fn make_toggle_message(on: bool) -> Message {
    let mut misc = Misc::new();
    misc.set_toggle_blackout(crate::message_proto::ToggleBlackout {
        on,
        ..Default::default()
    });
    let mut msg = Message::new();
    msg.set_misc(misc);
    msg
}

/// What one session's `ToggleBlackout` does. `allowed` is the host's verdict that the sender
/// is an authorized remote-control session with input permission. Repeating the current
/// state is a no-op that still answers `Ok`.
pub fn handle_toggle<L>(
    allowed: bool,
    on: bool,
    lease: &mut Option<L>,
    acquire: impl FnOnce() -> Result<L, BlackoutError>,
    release: impl FnOnce(L) -> Result<(), BlackoutError>,
) -> BlackoutResponse {
    if !allowed {
        return response(
            Status::Denied,
            lease.is_some(),
            "Remote control permission is required".to_owned(),
        );
    }
    if on == lease.is_some() {
        return response(Status::Ok, on, String::new());
    }
    if on {
        match acquire() {
            Ok(l) => {
                *lease = Some(l);
                response(Status::Ok, true, String::new())
            }
            Err(BlackoutError::NotSupported(s)) => response(Status::NotSupported, false, s),
            Err(BlackoutError::Failed(s)) => response(Status::Failed, false, s),
        }
    } else {
        let Some(l) = lease.take() else {
            return response(Status::Ok, false, String::new());
        };
        match release(l) {
            Ok(()) => response(Status::Ok, false, String::new()),
            Err(e) => response(Status::Failed, false, e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::Message as _;

    fn ok_acquire() -> Result<u8, BlackoutError> {
        Ok(7)
    }

    fn ok_release(_: u8) -> Result<(), BlackoutError> {
        Ok(())
    }

    fn status(r: &BlackoutResponse) -> Status {
        r.status.enum_value().unwrap()
    }

    #[test]
    fn on_then_off() {
        let mut lease = None;
        let r = handle_toggle(true, true, &mut lease, ok_acquire, ok_release);
        assert_eq!((status(&r), r.on, lease), (Status::Ok, true, Some(7)));
        let r = handle_toggle(true, false, &mut lease, ok_acquire, ok_release);
        assert_eq!((status(&r), r.on, lease), (Status::Ok, false, None));
    }

    #[test]
    fn repeated_on_and_off_do_not_touch_the_display() {
        let mut lease = Some(1u8);
        let r = handle_toggle(
            true,
            true,
            &mut lease,
            || -> Result<u8, BlackoutError> { panic!("acquired twice") },
            ok_release,
        );
        assert_eq!((status(&r), r.on, lease), (Status::Ok, true, Some(1)));
        let mut lease: Option<u8> = None;
        let r = handle_toggle(true, false, &mut lease, ok_acquire, |_| {
            panic!("released twice")
        });
        assert_eq!((status(&r), r.on), (Status::Ok, false));
    }

    #[test]
    fn unauthorized_sender_is_denied_and_keeps_state() {
        let mut lease: Option<u8> = None;
        let r = handle_toggle(false, true, &mut lease, || panic!("acquired"), ok_release);
        assert_eq!((status(&r), r.on, lease), (Status::Denied, false, None));
        let mut lease = Some(3u8);
        let r = handle_toggle(false, false, &mut lease, ok_acquire, |_| panic!("released"));
        assert_eq!((status(&r), r.on, lease), (Status::Denied, true, Some(3)));
    }

    #[test]
    fn unsupported_host_and_failure_never_report_on() {
        let mut lease: Option<u8> = None;
        let r = handle_toggle(
            true,
            true,
            &mut lease,
            || Err(BlackoutError::NotSupported("Wayland".to_owned())),
            ok_release,
        );
        assert_eq!(
            (status(&r), r.on, r.details.as_str()),
            (Status::NotSupported, false, "Wayland")
        );
        let r = handle_toggle(
            true,
            true,
            &mut lease,
            || Err(BlackoutError::Failed("BadMatch".to_owned())),
            ok_release,
        );
        assert_eq!((status(&r), r.on, lease), (Status::Failed, false, None));
    }

    #[test]
    fn toggle_round_trips_on_the_wire() {
        let bytes = make_toggle_message(true).write_to_bytes().unwrap();
        let msg = Message::parse_from_bytes(&bytes).unwrap();
        assert!(msg.misc().toggle_blackout().on);
        let bytes = make_message(response(Status::Failed, false, "x".to_owned()))
            .write_to_bytes()
            .unwrap();
        let r = Message::parse_from_bytes(&bytes)
            .unwrap()
            .misc()
            .blackout_response()
            .clone();
        assert_eq!(
            (status(&r), r.on, r.details.as_str()),
            (Status::Failed, false, "x")
        );
    }

    #[test]
    fn older_peers_ignore_blackout_and_report_no_capability() {
        // A schema without field 1001 still parses the bytes and sees no known field set,
        // which is what an older peer's `Misc` does with a blackout request.
        let misc_bytes = make_toggle_message(true).misc().write_to_bytes().unwrap();
        let legacy =
            crate::message_proto::TogglePrivacyMode::parse_from_bytes(&misc_bytes).unwrap();
        assert!(!legacy.on && legacy.impl_key.is_empty());
        // An old host's PeerInfo carries no blackout feature, which reads as unsupported.
        let old_features = crate::message_proto::Features {
            privacy_mode: true,
            ..Default::default()
        };
        let parsed = crate::message_proto::Features::parse_from_bytes(
            &old_features.write_to_bytes().unwrap(),
        )
        .unwrap();
        assert!(!parsed.blackout);
    }

    #[test]
    fn invalid_message_is_rejected() {
        assert!(Message::parse_from_bytes(&[0xff, 0xff, 0xff]).is_err());
    }
}
