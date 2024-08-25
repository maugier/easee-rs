use serde::Deserialize;
use serde_repr::Deserialize_repr;
use std::num::{ParseFloatError, ParseIntError};
use thiserror::Error;
use tracing::info;
use ureq::json;

use crate::{
    api::{ChargerOpMode, Context, UtcDateTime},
    signalr::{self, StreamError},
    stream::NegotiateError,
};

#[derive(Clone, Copy, Debug, Deserialize_repr)]
#[repr(u8)]
pub enum PilotMode {
    Disconnected = b'A',
    Connected = b'B',
    Charging = b'C',
    NeedsVentilation = b'D',
    FaultDetected = b'F',
    Unknown = b'\x00',
}

impl From<&str> for PilotMode {
    fn from(value: &str) -> Self {
        use PilotMode::*;
        match value {
            "A" => Disconnected,
            "B" => Connected,
            "C" => Charging,
            "D" => NeedsVentilation,
            "F" => FaultDetected,
            _ => Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize_repr)]
#[repr(u8)]
pub enum PhaseMode {
    Ignore = 0,
    Phase1 = 1,
    Auto = 2,
    Phase2 = 3,
}

#[derive(Clone, Copy, Debug)]
pub enum InputPin {
    T1,
    T2,
    T3,
    T4,
    T5,
}

#[derive(Clone, Copy, Debug, Deserialize_repr)]
#[repr(u8)]
enum DataType {
    Boolean = 2,
    Double = 3,
    Integer = 4,
    String = 6,
}

#[derive(Clone, Debug)]
pub enum ObservationData {
    Boolean(bool),
    Double(f64),
    Integer(i64),
    String(String),
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("integer `{0}`: {1}")]
    Integer(String, ParseIntError),

    #[error("double `{0}: {1}")]
    Double(String, ParseFloatError),
}

impl ObservationData {
    fn from_dynamic(value: String, data_type: DataType) -> Result<ObservationData, ParseError> {
        Ok(match data_type {
            DataType::Boolean => ObservationData::Boolean(
                value
                    .parse::<i64>()
                    .map_err(move |e| ParseError::Integer(value, e))?
                    != 0,
            ),
            DataType::Double => ObservationData::Double(
                value
                    .parse()
                    .map_err(move |e| ParseError::Double(value, e))?,
            ),
            DataType::Integer => ObservationData::Integer(
                value
                    .parse()
                    .map_err(move |e| ParseError::Integer(value, e))?,
            ),
            DataType::String => ObservationData::String(value),
        })
    }

    /*
    fn dynamic_type(&self) -> DataType {
        match self {
            ObservationData::Boolean(_) => DataType::Boolean,
            ObservationData::Double(_) => DataType::Double,
            ObservationData::Integer(_) => DataType::Integer,
            ObservationData::String(_) => DataType::String,
        }
    }
    */
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReasonForNoCurrent(u16);

impl std::fmt::Display for ReasonForNoCurrent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self.0 {
                0 => "OK",
                1 => "LoadBalance: circuit too low",
                2 => "LoadBalance: dynamic circuit too low",
                3 => "LoadBalance: max dynamic offline",
                4 => "LoadBalance: circuit fuse too low",
                5 => "LoadBalance: waiting in queue",
                6 => "LoadBalance: waiting in charged queue",
                7 => "Error: illegal grid type",
                8 => "Error: not received request from car",
                9 => "Error: master communication lost",
                10 => "Error: no current from equalizer",
                11 => "Error: no current, phase disconnected",
                25 => "Error: limited by circuit fuse",
                26 => "Error: limited by circuit max current",
                27 => "Error: limited by dynamic circuit current",
                28 => "Error: limited by equalizer",
                29 => "Error: limited by circuit load balancing",
                30 => "Error: limited by offline settings",
                53 => "Info: charger disabled",
                54 => "Waiting: pending schedule",
                55 => "Waiting: pending authorization",
                56 => "Error: charger in error state",
                57 => "Error: Erratic EV",
                75 => "Cable: limited by cable rating",
                76 => "Schedule: limited by schedule",
                77 => "Charger limit: limited by charger max current",
                78 => "Charger Limit: limited by dynamic charger current",
                79 => "Car limit: limited by car not charging",
                80 => "Local: limited by local adjustment",
                81 => "Car limit: limited by car",
                100 => "Error: undefined",
                other => return write!(f, "Code {other}"),
            }
        )
    }
}

#[derive(Debug)]
pub enum Observation {
    SelfTestResult(String),
    SelfTestDetails(String),
    WifiEvent(i64),
    ChargerOfflineReason(i64),
    CircuitMaxCurrent { phase: u8, amperes: i64 },
    SiteID(String),
    IsEnabled(bool),
    Temperature(i64),
    TriplePhase(bool),
    DynamicChargerCurrent(f64),

    ICCID(String),
    MobileNetworkOperator(String),

    ReasonForNoCurrent(ReasonForNoCurrent),
    PilotMode(PilotMode),
    SmartCharging(bool),
    CableLocked(bool),
    CableRating(f64),
    UserId(String),
    ChargerOpMode(ChargerOpMode),
    IntCurrent { pin: InputPin, current: f64 },

    TotalPower(f64),
    EnergyPerHour(f64),
    LifetimeEnergy(f64),

    Unknown { code: u16, value: ObservationData },
}

fn op_mode_from_int(mode: i64) -> ChargerOpMode {
    use ChargerOpMode::*;
    match mode {
        1 => Disconnected,
        2 => Paused,
        3 => Charging,
        4 => Finished,
        5 => Error,
        6 => Ready,
        _ => Unknown,
    }
}

impl Observation {
    fn try_from_data(code: u16, data: ObservationData) -> Observation {
        use InputPin::*;
        use Observation::*;
        use ObservationData::*;
        match (code, data) {
            (1, String(result)) => SelfTestResult(result),
            (2, String(details)) => SelfTestDetails(details),
            (10, Integer(wifi)) => WifiEvent(wifi),
            (11, Integer(reason)) => ChargerOfflineReason(reason),
            (22, Integer(amperes)) => CircuitMaxCurrent { phase: 1, amperes },
            (23, Integer(amperes)) => CircuitMaxCurrent { phase: 2, amperes },
            (24, Integer(amperes)) => CircuitMaxCurrent { phase: 3, amperes },
            (26, String(site)) => SiteID(site),
            (31, Boolean(enabled)) => IsEnabled(enabled),
            (32, Integer(temperature)) => Temperature(temperature),
            (38, Integer(1)) => TriplePhase(false),
            (38, Integer(3)) => TriplePhase(true),
            (48, Double(current)) => DynamicChargerCurrent(current),
            (81, String(iccid)) => ICCID(iccid),
            (84, String(operator)) => MobileNetworkOperator(operator),
            (96, Integer(reason)) => ReasonForNoCurrent(self::ReasonForNoCurrent(reason as u16)),
            (100, String(l)) => PilotMode(super::observation::PilotMode::from(&*l)),
            (102, Boolean(enabled)) => SmartCharging(enabled),
            (103, Boolean(locked)) => CableLocked(locked),
            (104, Double(amps)) => CableRating(amps),
            (107, String(tok_rev)) => UserId(tok_rev.chars().rev().collect()),
            (109, Integer(mode)) => ChargerOpMode(op_mode_from_int(mode)),
            (120, Double(power)) => TotalPower(power),
            (182, Double(current)) => IntCurrent { pin: T2, current },
            (183, Double(current)) => IntCurrent { pin: T3, current },
            (184, Double(current)) => IntCurrent { pin: T4, current },
            (185, Double(current)) => IntCurrent { pin: T5, current },

            (code, value) => Unknown { code, value },
        }
    }
}

#[derive(Debug)]
pub struct Event {
    pub charger: String,
    pub observation: Observation,
}

pub struct Stream {
    inner: signalr::Stream,
}

#[derive(Debug, Error)]
pub enum ObservationError {
    #[error("stream: {0}")]
    Stream(#[from] StreamError),

    #[error("Protocol error")]
    Protocol(signalr::Message),

    #[error("JSON: {0}")]
    Deserialize(#[from] serde_json::Error),

    #[error("Parsing: {0}")]
    Parsing(#[from] ParseError),
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ProductUpdate {
    data_type: DataType,
    id: u16,
    mid: String,
    timestamp: UtcDateTime,
    value: String,
}

impl Stream {
    pub fn from_context(ctx: &mut Context) -> Result<Self, NegotiateError> {
        Ok(Self {
            inner: signalr::Stream::from_ws(crate::stream::Stream::open(ctx)?),
        })
    }

    pub fn recv(&mut self) -> Result<Event, ObservationError> {
        use signalr::Message::*;
        let de = |msg| -> Result<Event, ObservationError> { Err(ObservationError::Protocol(msg)) };
        loop {
            let msg = self.inner.recv()?;
            match &msg {
                Ping => continue,
                Empty | InvocationResult { .. } => info!("Skipped message: {msg:?}"),
                Invocation { target, arguments } if target == "ProductUpdate" => {
                    if arguments.len() != 1 {
                        return de(msg);
                    };
                    let evt = ProductUpdate::deserialize(&arguments[0])?;
                    return decode_update(evt);
                }
                Invocation { .. } => continue,
                _other => return de(msg),
            }
        }
    }
    pub fn subscribe(&mut self, id: &str) -> Result<(), tungstenite::Error> {
        self.inner
            .invoke("SubscribeWithCurrentState", json!([id, true]))
    }
}

fn decode_update(update: ProductUpdate) -> Result<Event, ObservationError> {
    let ProductUpdate {
        data_type,
        id,
        mid,
        timestamp,
        value,
    } = update;
    let data = ObservationData::from_dynamic(value, data_type)?;
    let obs = Observation::try_from_data(id, data);
    let _ = timestamp;
    Ok(Event {
        charger: mid,
        observation: obs,
    })
}
