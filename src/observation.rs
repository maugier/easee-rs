use crate::api::ChargerOpMode;

#[repr(u8)]
pub enum PilotMode {
    Disconnected = b'A',
    Connected = b'B',
    Charging = b'C',
    NeedsVentilation = b'D',
    FaultDetected = b'F',
}

#[repr(u8)]
pub enum PhaseMode {
    Ignore = 0,
    Phase1 = 1,
    Auto = 2,
    Phase2 = 3,
}

pub enum ReasonForNoCurrent {}

pub enum Observation {
    SelfTestResult(String),
    SelfTestDetails(serde_json::Value),
    WifiEvent(u64),
    ChargerOfflineReason(u64),
    CircuitMaxCurrent { phase: u8, amperes: u64 },
    SiteID(String),
    IsEnabled(bool),
    Temperature(u64),
    TriplePhase(bool),
    DynamicChargerCurrent(f64),
    ReasonForNoCurrent(ReasonForNoCurrent),
    PilotMode(PilotMode),
    SmartCharging(bool),
    CableLocked(bool),
    CableRating(f64),
    UserId(String),
    ChargerOpMode(ChargerOpMode),
}
