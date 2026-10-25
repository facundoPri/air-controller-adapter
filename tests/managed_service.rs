use ready_for_air::service::{Handshake, PROTOCOL_VERSION};

#[test]
fn an_old_reader_cannot_be_used_after_an_app_update() {
    let expected = Handshake::current();
    assert!(expected.accepts(PROTOCOL_VERSION, &expected.build).is_ok());
    assert!(
        expected
            .accepts(PROTOCOL_VERSION, "previous-build")
            .is_err()
    );
    assert!(
        expected
            .accepts(PROTOCOL_VERSION + 1, &expected.build)
            .is_err()
    );
}

#[test]
fn denied_approval_requires_settings_and_never_triggers_registration_again() {
    use ready_for_air::service::{Setup, Status, Step};
    let mut setup = Setup::default();
    assert_eq!(setup.advance(Status::NotRegistered), Step::Register);
    assert_eq!(setup.advance(Status::RequiresApproval), Step::OpenSettings);
    assert_eq!(setup.advance(Status::RequiresApproval), Step::OpenSettings);
    assert_eq!(setup.advance(Status::Enabled), Step::Connect);
    let mut canceled = Setup::default();
    assert_eq!(canceled.advance(Status::NotRegistered), Step::Register);
    assert_eq!(canceled.advance(Status::NotRegistered), Step::Wait);
}

#[test]
fn a_verified_package_can_be_registered_when_macos_has_no_record_yet() {
    use ready_for_air::service::{Setup, Status, Step};
    let mut setup = Setup::default();
    assert_eq!(setup.advance(Status::NotFound), Step::Register);
    assert_eq!(setup.advance(Status::NotFound), Step::Wait);
}

#[test]
fn a_live_reader_has_no_session_expiry_but_lost_heartbeats_release_it() {
    use ready_for_air::service::CaptureLease;
    use std::time::Duration;
    let mut lease = CaptureLease::new(Duration::ZERO);
    for second in 1..=7200 {
        let now = Duration::from_secs(second);
        lease.check(now, true).unwrap();
        lease.heartbeat(now);
    }
    assert!(lease.check(Duration::from_secs(7203), true).is_err());
    assert!(lease.check(Duration::from_secs(7200), false).is_err());
}
