//! UI contract. Transport observations, readiness and user intent are separate.
use crate::{permissions, studio::MouseMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Readiness {
    NotConnected,
    Connecting,
    Preparing,
    Ready,
    NeedsSetup,
    Disconnecting,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetupAction {
    StudioInput,
    CaptureInput,
    RequestReaderInput,
    Accessibility,
    BluetoothPermission,
    BluetoothSettings,
    AuthorizeReader,
    EnableService,
    ApproveService,
    RetryReader,
    SelectDevice,
    RepairConfiguration,
    Connect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportEvidence {
    Unknown,
    HidPresent,
    NotDetected,
    LocalBluetoothConnected,
    /// ACA has stopped observing; this does not assert that Bluetooth is closed.
    UnverifiedAfterStop,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceIdentity {
    /// Opaque `hid:` path or platform BLE identifier. Never substitute a name for identity.
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisconnectCapability {
    /// Stop ACA first, then ask the user to disconnect this remote in Settings.
    SystemSettingsHandoff,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionStatus {
    pub state: Readiness,
    pub device: Option<DeviceIdentity>,
    pub candidates: Vec<DeviceIdentity>,
    pub observed_mode: Option<MouseMode>,
    pub transport: TransportEvidence,
    pub reason: String,
    pub next_action: Option<SetupAction>,
    pub disconnect: DisconnectCapability,
    pub reconnect_allowed: bool,
    pub permissions: permissions::Snapshot,
    pub service_status: crate::service::Status,
    pub service_error: Option<String>,
    pub capture_access: permissions::Access,
    pub capture_authenticated: bool,
    pub exclusive_reader: bool,
    pub reading_verified: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionCommand {
    Connect,
    Disconnect,
    SelectDevice(String),
    ResolveSetup,
    RetrySetup,
    RequestReaderInput,
    EnableService,
    RemoveService,
}
