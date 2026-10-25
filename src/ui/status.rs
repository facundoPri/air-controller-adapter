//! Presentation of the core's `ConnectionStatus`: one headline, the next
//! step and whether Disconnect applies. Pure so every state can be tested;
//! readiness itself is decided by the core, never re-derived here.

use super::theme::{ACCENT, DANGER, FAINT, WARN, mode_name};
use ready_for_air::lifecycle::{
    ConnectionCommand, ConnectionStatus, Readiness, SetupAction, TransportEvidence,
};

/// What a setup button does when pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Run(ConnectionCommand),
    /// Show the offered remotes; selection needs an explicit choice.
    ChooseDevice,
    /// Open the configuration editor in Connection.
    OpenConfiguration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextStep {
    pub label: &'static str,
    pub step: Step,
}

fn next(label: &'static str, step: Step) -> Option<NextStep> {
    Some(NextStep { label, step })
}

/// ACA was stopped by the user and will not reconnect until Connect.
pub fn stopped(s: &ConnectionStatus) -> bool {
    !s.reconnect_allowed && s.state == Readiness::NeedsSetup
}

/// Short state label and its colour. Only the core's `Ready` reads as ready.
pub fn headline(s: &ConnectionStatus) -> (&'static str, u32) {
    if stopped(s) {
        return ("Stopped", FAINT);
    }
    match s.state {
        Readiness::Ready => ("Ready", ACCENT),
        Readiness::Connecting => ("Connecting…", WARN),
        Readiness::Preparing => ("Preparing…", WARN),
        Readiness::NeedsSetup => ("Needs setup", WARN),
        Readiness::Disconnecting => ("Stopping…", WARN),
        Readiness::Failed => ("Needs attention", DANGER),
        Readiness::NotConnected => ("Not connected", FAINT),
    }
}

/// Observed hardware mode, absent until input has established it.
pub fn observed_mode(s: &ConnectionStatus) -> Option<&'static str> {
    s.observed_mode.map(mode_name)
}

/// The one action that resolves the current state, if any.
pub fn primary(s: &ConnectionStatus) -> Option<NextStep> {
    if stopped(s) {
        return next("Connect", Step::Run(ConnectionCommand::Connect));
    }
    let resolve = || Step::Run(ConnectionCommand::ResolveSetup);
    match s.next_action? {
        SetupAction::StudioInput | SetupAction::CaptureInput => {
            next("Open Input Monitoring", resolve())
        }
        SetupAction::RequestReaderInput => next(
            "Request reader access",
            Step::Run(ConnectionCommand::RequestReaderInput),
        ),
        SetupAction::Accessibility => next("Open Accessibility", resolve()),
        SetupAction::BluetoothPermission => next("Allow Bluetooth", resolve()),
        SetupAction::BluetoothSettings => next("Open Bluetooth Settings", resolve()),
        SetupAction::EnableService => next("Enable ACA", resolve()),
        SetupAction::ApproveService => next("Open Login Items", resolve()),
        SetupAction::AuthorizeReader => next("Authorize reader", resolve()),
        SetupAction::RetryReader => next("Retry reader", resolve()),
        SetupAction::SelectDevice => next("Choose remote", Step::ChooseDevice),
        SetupAction::RepairConfiguration => next("Open configuration", Step::OpenConfiguration),
        SetupAction::Connect => next("Connect", Step::Run(ConnectionCommand::Connect)),
    }
}

/// A follow-up that belongs next to the primary step.
pub fn secondary(s: &ConnectionStatus) -> Option<NextStep> {
    match s.next_action? {
        // Physical disconnection is a Settings handoff after ACA stops.
        SetupAction::BluetoothSettings if stopped(s) => next(
            "Bluetooth Settings",
            Step::Run(ConnectionCommand::ResolveSetup),
        ),
        // Capture's permission is granted in Settings, then needs a new reader.
        SetupAction::CaptureInput => next("Retry reader", Step::Run(ConnectionCommand::RetrySetup)),
        _ => None,
    }
}

/// Disconnect stops ACA for this run; it is offered while ACA wants the remote.
pub fn can_disconnect(s: &ConnectionStatus) -> bool {
    s.reconnect_allowed && !matches!(s.state, Readiness::NotConnected | Readiness::Disconnecting)
}

/// The Buttons view leads with connection guidance instead of the editor.
pub fn landing(s: &ConnectionStatus) -> bool {
    s.state == Readiness::NotConnected || stopped(s)
}

/// First-time setup, in the order a new user meets it.
pub struct Requirement {
    pub title: &'static str,
    pub detail: &'static str,
    pub met: bool,
}

pub fn checklist(s: &ConnectionStatus) -> [Requirement; 3] {
    [
        Requirement {
            title: "Wake or pair the remote",
            detail: "Press any button to wake a paired remote. To pair, hold Back + OK until the LED blinks, then choose Connect. If a phone grabs the remote, turn off the phone's Bluetooth first.",
            met: matches!(
                s.transport,
                TransportEvidence::HidPresent | TransportEvidence::LocalBluetoothConnected
            ),
        },
        Requirement {
            title: if cfg!(feature = "managed-reader") {
                "Allow ACA"
            } else {
                "Allow ACA Studio"
            },
            detail: "Input Monitoring reads the remote; Accessibility performs its actions. Grant both in Privacy & Security.",
            met: s.permissions.input.allowed() && s.permissions.accessibility,
        },
        Requirement {
            title: if cfg!(feature = "managed-reader") {
                "Enable ACA"
            } else {
                "Authorize the reader"
            },
            detail: if cfg!(feature = "managed-reader") {
                "Enable ACA once in Login Items & Extensions. Choose Request reader access, then follow the entry macOS identifies in Privacy & Security. Press a button to verify the connection."
            } else {
                "ACA Capture asks for administrator approval once per session. Sessions last up to 30 minutes; renew one here when it ends."
            },
            met: s.capture_authenticated
                && (!cfg!(feature = "managed-reader") || s.capture_access.allowed()),
        },
    ]
}

pub fn transport(t: TransportEvidence) -> &'static str {
    match t {
        TransportEvidence::Unknown => "Not checked yet",
        TransportEvidence::HidPresent => "Remote present (HID)",
        TransportEvidence::NotDetected => "Not detected",
        TransportEvidence::LocalBluetoothConnected => "Connected by ACA (Bluetooth)",
        TransportEvidence::UnverifiedAfterStop => "Unverified · ACA stopped observing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ready_for_air::{
        lifecycle::DisconnectCapability,
        permissions::{Access, Snapshot},
    };

    fn status(state: Readiness, next_action: Option<SetupAction>) -> ConnectionStatus {
        ConnectionStatus {
            state,
            device: None,
            candidates: vec![],
            observed_mode: None,
            transport: TransportEvidence::HidPresent,
            reason: String::new(),
            next_action,
            disconnect: DisconnectCapability::SystemSettingsHandoff,
            reconnect_allowed: true,
            permissions: Snapshot {
                input: Access::Granted,
                accessibility: true,
                bluetooth: Access::Granted,
            },
            service_status: Default::default(),
            service_error: None,
            capture_access: Access::Granted,
            capture_authenticated: true,
            exclusive_reader: true,
            reading_verified: false,
        }
    }

    // gpui_kit's glob import shadows the std `test` attribute.
    #[::core::prelude::v1::test]
    fn only_core_ready_reads_as_ready() {
        for state in [
            Readiness::NotConnected,
            Readiness::Connecting,
            Readiness::Preparing,
            Readiness::NeedsSetup,
            Readiness::Disconnecting,
            Readiness::Failed,
        ] {
            // Transport and permission evidence alone never make it ready.
            let s = status(state, None);
            assert_ne!(headline(&s).0, "Ready", "{state:?}");
            assert_ne!(headline(&s).1, ACCENT, "{state:?}");
        }
        assert_eq!(headline(&status(Readiness::Ready, None)), ("Ready", ACCENT));
    }

    #[::core::prelude::v1::test]
    fn stopped_keeps_connect_and_the_settings_handoff() {
        let mut s = status(Readiness::NeedsSetup, Some(SetupAction::BluetoothSettings));
        s.reconnect_allowed = false;
        s.transport = TransportEvidence::UnverifiedAfterStop;
        assert_eq!(headline(&s).0, "Stopped");
        assert_eq!(
            primary(&s).unwrap().step,
            Step::Run(ConnectionCommand::Connect)
        );
        assert_eq!(
            secondary(&s).unwrap().step,
            Step::Run(ConnectionCommand::ResolveSetup)
        );
        assert!(landing(&s) && !can_disconnect(&s));
        // A discovery failure that points to Settings is not a user stop.
        let failed = status(Readiness::Failed, Some(SetupAction::BluetoothSettings));
        assert_eq!(primary(&failed).unwrap().label, "Open Bluetooth Settings");
        assert!(secondary(&failed).is_none() && can_disconnect(&failed));
    }

    #[::core::prelude::v1::test]
    fn pending_states_offer_no_setup_step_and_stopping_hides_disconnect() {
        for state in [
            Readiness::Connecting,
            Readiness::Preparing,
            Readiness::Ready,
        ] {
            assert!(primary(&status(state, None)).is_none());
            assert!(can_disconnect(&status(state, None)));
        }
        let stopping = status(Readiness::Disconnecting, None);
        assert!(primary(&stopping).is_none() && !can_disconnect(&stopping));
        assert!(!landing(&stopping));
    }

    #[::core::prelude::v1::test]
    fn each_setup_requirement_maps_to_its_own_step() {
        let step = |a| primary(&status(Readiness::NeedsSetup, Some(a))).unwrap();
        assert_eq!(step(SetupAction::SelectDevice).step, Step::ChooseDevice);
        assert_eq!(
            step(SetupAction::RepairConfiguration).step,
            Step::OpenConfiguration
        );
        assert_eq!(
            step(SetupAction::AuthorizeReader).step,
            Step::Run(ConnectionCommand::ResolveSetup)
        );
        assert_eq!(
            secondary(&status(
                Readiness::NeedsSetup,
                Some(SetupAction::CaptureInput)
            ))
            .unwrap()
            .step,
            Step::Run(ConnectionCommand::RetrySetup)
        );
        let not_connected = status(Readiness::NotConnected, Some(SetupAction::Connect));
        assert_eq!(
            primary(&not_connected).unwrap().step,
            Step::Run(ConnectionCommand::Connect)
        );
        assert!(landing(&not_connected) && !can_disconnect(&not_connected));
    }

    #[::core::prelude::v1::test]
    fn undetermined_reader_requests_access_and_denied_reader_opens_settings() {
        let mut s = status(Readiness::NeedsSetup, Some(SetupAction::RequestReaderInput));
        s.capture_access = Access::Unknown;
        assert_eq!(
            primary(&s).unwrap(),
            NextStep {
                label: "Request reader access",
                step: Step::Run(ConnectionCommand::RequestReaderInput),
            }
        );
        assert!(secondary(&s).is_none());
        if cfg!(feature = "managed-reader") {
            assert!(!checklist(&s)[2].met);
        }

        s.capture_access = Access::Denied;
        s.next_action = Some(SetupAction::CaptureInput);
        assert_eq!(primary(&s).unwrap().label, "Open Input Monitoring");
        assert_eq!(
            primary(&s).unwrap().step,
            Step::Run(ConnectionCommand::ResolveSetup)
        );
        assert_eq!(
            secondary(&s).unwrap().step,
            Step::Run(ConnectionCommand::RetrySetup)
        );

        s.capture_access = Access::Unknown;
        s.next_action = Some(SetupAction::RequestReaderInput);
        s.reconnect_allowed = false;
        assert_eq!(
            primary(&s).unwrap().step,
            Step::Run(ConnectionCommand::Connect)
        );
    }
}
