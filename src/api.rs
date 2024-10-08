use std::{
    io,
    ops::{Add, Mul, Sub},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_repr::Deserialize_repr;
use thiserror::Error;
use tracing::{debug, info, instrument};

pub struct Context {
    auth_header: String,
    refresh_token: String,
    token_expiration: Instant,
    on_refresh: Option<Box<dyn FnMut(&mut Self) + Send>>,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("auth_header", &"<secret>")
            .field("refresh_token", &"<secret>")
            .field("token_expiration", &self.token_expiration)
            .field("on_refresh", &"[closure]")
            .finish()
    }
}

const API_BASE: &str = "https://api.easee.com/api/";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NaiveDateTime(pub chrono::NaiveDateTime);

impl<'de> Deserialize<'de> for NaiveDateTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = <&str as Deserialize>::deserialize(d)?;
        let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
            .map_err(D::Error::custom)?;
        Ok(NaiveDateTime(dt))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UtcDateTime(pub chrono::DateTime<chrono::Utc>);

impl<'de> Deserialize<'de> for UtcDateTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = <&str as Deserialize>::deserialize(d)?;
        let dt = chrono::DateTime::parse_from_str(s, "%+")
            .map_err(D::Error::custom)?
            .to_utc();
        Ok(UtcDateTime(dt))
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Triphase {
    pub phase1: f64,
    pub phase2: f64,
    pub phase3: f64,
}

impl Add<Triphase> for Triphase {
    type Output = Triphase;

    fn add(self, rhs: Triphase) -> Self::Output {
        Triphase {
            phase1: self.phase1 + rhs.phase1,
            phase2: self.phase2 + rhs.phase2,
            phase3: self.phase3 + rhs.phase3,
        }
    }
}

impl Sub<Triphase> for Triphase {
    type Output = Triphase;

    fn sub(self, rhs: Triphase) -> Self::Output {
        Triphase {
            phase1: self.phase1 + rhs.phase1,
            phase2: self.phase2 + rhs.phase2,
            phase3: self.phase3 + rhs.phase3,
        }
    }
}

impl Mul<f64> for Triphase {
    type Output = Triphase;

    fn mul(self, rhs: f64) -> Self::Output {
        Triphase {
            phase1: self.phase1 * rhs,
            phase2: self.phase2 * rhs,
            phase3: self.phase3 * rhs,
        }
    }
}

impl From<f64> for Triphase {
    fn from(value: f64) -> Self {
        Triphase {
            phase1: value,
            phase2: value,
            phase3: value,
        }
    }
}

#[derive(Clone, Copy, Serialize)]
pub struct SetCurrent {
    pub time_to_live: Option<i32>,
    #[serde(flatten)]
    pub current: Triphase,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct Charger {
    pub id: String,
    pub name: String,
    pub product_code: u32,
    pub color: Option<i32>,
    pub created_on: NaiveDateTime,
    pub updated_on: NaiveDateTime,
    pub level_of_access: u32,
}

#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ChargerOpMode {
    Unknown = 0,
    Disconnected = 1,
    Paused = 2,
    Charging = 3,
    Finished = 4,
    Error = 5,
    Ready = 6,
    AwaitingAuthentication = 7,
    Deauthenticating = 8,
}

#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum OutputPhase {
    Unknown = 0,
    L1ToN = 10,
    L2ToN = 12,
    L3ToN = 14,
    L1ToL2 = 11,
    L2ToL3 = 15,
    L3ToL1 = 13,
    L1L2ToN = 20,
    L2L3ToN = 21,
    L1L3ToL2 = 22,
    L1L2L3ToN = 30,
}

#[derive(Clone, Debug, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct ChargerState {
    pub smart_charging: bool,
    pub cable_locked: bool,
    pub charger_op_mode: ChargerOpMode,
    pub total_power: f64,
    pub session_energy: f64,
    pub energy_per_hour: f64,

    #[serde(rename = "wiFiRSSI")]
    pub wifi_rssi: Option<i32>,

    #[serde(rename = "cellRSSI")]
    pub cell_rssi: Option<i32>,

    #[serde(rename = "localRSSI")]
    pub local_rssi: Option<i32>,
    pub output_phase: OutputPhase,
    pub dynamic_circuit_current_p1: u32,
    pub dynamic_circuit_current_p2: u32,
    pub dynamic_circuit_current_p3: u32,

    pub latest_pulse: UtcDateTime,
    pub charger_firmware: u32,
    pub voltage: f64,

    #[serde(rename = "chargerRAT")]
    pub charger_rat: u32,
    pub lock_cable_permanently: bool,
    pub in_current_t2: Option<f64>,
    pub in_current_t3: Option<f64>,
    pub in_current_t4: Option<f64>,
    pub in_current_t5: Option<f64>,
    pub output_current: f64,
    pub is_online: bool,
    pub in_voltage_t1_t2: Option<f64>,
    pub in_voltage_t1_t3: Option<f64>,
    pub in_voltage_t1_t4: Option<f64>,
    pub in_voltage_t1_t5: Option<f64>,
    pub in_voltage_t2_t3: Option<f64>,
    pub in_voltage_t2_t4: Option<f64>,
    pub in_voltage_t2_t5: Option<f64>,
    pub in_voltage_t3_t4: Option<f64>,
    pub in_voltage_t3_t5: Option<f64>,
    pub in_voltage_t4_t5: Option<f64>,
    pub led_mode: u32,
    pub cable_rating: f64,
    pub dynamic_charger_current: f64,
    pub circuit_total_allocated_phase_conductor_current_l1: f64,
    pub circuit_total_allocated_phase_conductor_current_l2: f64,
    pub circuit_total_allocated_phase_conductor_current_l3: f64,
    pub circuit_total_phase_conductor_current_l1: f64,
    pub circuit_total_phase_conductor_current_l2: f64,
    pub circuit_total_phase_conductor_current_l3: f64,
    pub reason_for_no_current: u32,

    #[serde(rename = "wiFiAPEnabled")]
    pub wifi_ap_enabled: bool,
    pub lifetime_energy: f64,
    pub offline_max_circuit_current_p1: u32,
    pub offline_max_circuit_current_p2: u32,
    pub offline_max_circuit_current_p3: u32,
    pub error_code: u32,
    pub fatal_error_code: u32,
    pub eq_available_current_p1: Option<f64>,
    pub eq_available_current_p2: Option<f64>,
    pub eq_available_current_p3: Option<f64>,
    pub derated_current: Option<f64>,
    pub derating_active: bool,
    pub connected_to_cloud: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct ChargingSession {
    pub charger_id: Option<String>,
    pub session_energy: f64,
    //pub session_start: Option<NaiveDateTime>,
    //pub session_stop: Option<NaiveDateTime>,
    pub session_id: Option<i32>,
    pub charge_duration_in_seconds: Option<u32>,
    //pub first_energy_transfer_period_start: Option<NaiveDateTime>,
    //pub last_energy_transfer_period_end: Option<NaiveDateTime>,
    #[serde(rename = "pricePrKwhIncludingVat")]
    pub price_per_kwh_including_vat: Option<f64>,
    pub price_per_kwh_excluding_vat: Option<f64>,
    pub vat_percentage: Option<f64>,
    pub currency_id: Option<String>,
    pub cost_including_vat: Option<f64>,
    pub cost_excluding_vat: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub uuid: Option<String>,
    pub id: u32,
    pub site_key: Option<String>,
    pub name: Option<String>,
    pub level_of_access: u32,
    //pub address: Address,
    pub installer_alias: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteDetails {
    #[serde(flatten)]
    pub site: Site,
    pub circuits: Vec<Circuit>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Circuit {
    pub id: u32,
    pub uuid: String,
    pub site_id: u32,
    pub circuit_panel_id: i64,
    pub panel_name: String,
    pub rated_current: f64,
    pub fuse: f64,
    pub chargers: Vec<Charger>,
    pub use_dynamic_master: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub access_token: String,
    pub expires_in: u32,
    pub access_claims: Vec<Option<String>>,
    pub token_type: Option<String>,
    pub refresh_token: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandReply {
    command_id: u64,
    device: String,
    ticks: u64,
}

#[derive(Debug, Error)]
pub enum ApiError {
    /// HTTP call caused an IO error
    #[error("io: {0}")]
    IO(#[from] io::Error),

    /// HTTP call failed (404, etc)
    #[error("ureq")]
    Ureq(#[source] Box<ureq::Error>),

    /// HTTP call succeeded but the returned JSON document didn't match the expected format
    #[error("unexpected data: {1} when processing {0}")]
    UnexpectedData(serde_json::Value, serde_json::Error),

    /// A JSON datetime field did not contain a string
    #[error("could not deserialize time string")]
    DeserializeFail,

    /// A JSON datetime field could not be parsed
    #[error("format error: {0}")]
    FormatError(#[from] chrono::ParseError),

    #[error("Invalid ID: {0:?}")]
    InvalidID(String),
}

impl From<ureq::Error> for ApiError {
    fn from(value: ureq::Error) -> Self {
        ApiError::Ureq(Box::new(value))
    }
}

trait JsonExplicitError {
    /// Explicitely report the received JSON object we failed to parse
    fn into_json_with_error<T: DeserializeOwned>(self) -> Result<T, ApiError>;
}

impl JsonExplicitError for ureq::Response {
    fn into_json_with_error<T: DeserializeOwned>(self) -> Result<T, ApiError> {
        let resp: serde_json::Value = self.into_json()?;
        let parsed = T::deserialize(&resp);
        parsed.map_err(|e| ApiError::UnexpectedData(resp, e))
    }
}

#[derive(Debug, Error)]
pub enum TokenParseError {
    #[error("Bad line count")]
    IncorrectLineCount,

    #[error("Parse error: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),
}

impl Context {
    fn from_login_response(resp: LoginResponse) -> Self {
        Self {
            auth_header: format!("Bearer {}", &resp.access_token),
            refresh_token: resp.refresh_token,
            token_expiration: (Instant::now() + Duration::from_secs(resp.expires_in as u64)),
            on_refresh: None,
        }
    }

    pub fn from_saved(saved: &str) -> Result<Self, TokenParseError> {
        let lines: Vec<&str> = saved.lines().collect();
        let &[token, refresh, expire] = &*lines else {
            return Err(TokenParseError::IncorrectLineCount);
        };

        let expire: u64 = expire.parse()?;
        let token_expiration = Instant::now()
            + (UNIX_EPOCH + Duration::from_secs(expire))
                .duration_since(SystemTime::now())
                .unwrap_or_default();

        Ok(Self {
            auth_header: format!("Bearer {}", token),
            refresh_token: refresh.to_owned(),
            token_expiration,
            on_refresh: None,
        })
    }

    pub fn on_refresh<F: FnMut(&mut Self) + Send + 'static>(mut self, on_refresh: F) -> Self {
        self.on_refresh = Some(Box::new(on_refresh));
        self
    }

    pub fn save(&self) -> String {
        let expiration = (SystemTime::now() + (self.token_expiration - Instant::now()))
            .duration_since(UNIX_EPOCH)
            .unwrap();
        format!(
            "{}\n{}\n{}\n",
            self.auth_token(),
            self.refresh_token,
            expiration.as_secs()
        )
    }

    /// Retrieve access tokens online, by logging in with the provided credentials
    pub fn from_login(user: &str, password: &str) -> Result<Self, ApiError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'t> {
            user_name: &'t str,
            password: &'t str,
        }

        info!("Logging into API");
        let url: String = format!("{}accounts/login", API_BASE);
        let resp: LoginResponse = ureq::post(&url)
            .send_json(Params {
                user_name: user,
                password,
            })?
            .into_json_with_error()?;

        Ok(Self::from_login_response(resp))
    }

    /// Check if the token has reached its expiration date
    fn check_expired(&mut self) -> Result<(), ApiError> {
        if self.token_expiration < Instant::now() {
            debug!("Token has expired");
            self.refresh_token()?;
        }
        Ok(())
    }

    pub(crate) fn auth_token(&self) -> &str {
        &self.auth_header[7..]
    }

    /// Use the refresh token to refresh credentials
    pub fn refresh_token(&mut self) -> Result<(), ApiError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'t> {
            refresh_token: &'t str,
        }

        info!("Refreshing access token");
        let params = Params {
            refresh_token: &self.refresh_token,
        };
        let url = format!("{}accounts/refresh_token", API_BASE);
        let resp: LoginResponse = ureq::post(&url)
            .set("Content-type", "application/json")
            .send_json(params)?
            .into_json_with_error()?;

        *self = Self::from_login_response(resp);
        Ok(())
    }

    /// List all sites available to the user
    pub fn sites(&mut self) -> Result<Vec<Site>, ApiError> {
        self.get("sites")
    }

    pub fn site(&mut self, id: i32) -> Result<SiteDetails, ApiError> {
        self.get(&format!("sites/{id}"))
    }

    /// List all chargers available to the user
    pub fn chargers(&mut self) -> Result<Vec<Charger>, ApiError> {
        self.get("chargers")
    }

    pub fn charger(&mut self, id: &str) -> Result<Charger, ApiError> {
        if !id.chars().all(char::is_alphanumeric) {
            return Err(ApiError::InvalidID(id.to_owned()));
        }
        self.get(&format!("chargers/{}", id))
    }

    pub fn circuit(&mut self, site_id: u32, circuit_id: u32) -> Result<Circuit, ApiError> {
        self.get(&format!("site/{site_id}/circuit/{circuit_id}"))
    }

    pub fn circuit_dynamic_current(
        &mut self,
        site_id: u32,
        circuit_id: u32,
    ) -> Result<Triphase, ApiError> {
        self.get(&format!(
            "sites/{site_id}/circuits/{circuit_id}/dynamicCurrent"
        ))
    }

    pub fn set_circuit_dynamic_current(
        &mut self,
        site_id: u32,
        circuit_id: u32,
        current: SetCurrent,
    ) -> Result<(), ApiError> {
        self.post(
            &format!("sites/{site_id}/circuits/{circuit_id}/dynamicCurrent"),
            &current,
        )
    }

    #[instrument]
    fn get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T, ApiError> {
        self.check_expired()?;
        let url: String = format!("{}{}", API_BASE, path);
        let req = ureq::get(&url)
            .set("Accept", "application/json")
            .set("Authorization", &self.auth_header);

        let mut resp = req.clone().call()?;

        if resp.status() == 401 {
            self.refresh_token()?;
            resp = req.call()?
        }

        resp.into_json_with_error()
    }

    fn maybe_get<T: DeserializeOwned>(&mut self, path: &str) -> Result<Option<T>, ApiError> {
        match self.get(path) {
            Ok(r) => Ok(Some(r)),
            Err(ApiError::Ureq(e)) => match &*e {
                ureq::Error::Status(404, _) => Ok(None),
                _ => Err(ApiError::Ureq(e)),
            },
            Err(other) => Err(other),
        }
    }

    pub(crate) fn post<T: DeserializeOwned, P: Serialize>(
        &mut self,
        path: &str,
        params: &P,
    ) -> Result<T, ApiError> {
        let url: String = format!("{}{}", API_BASE, path);
        self.post_raw(&url, params)
    }

    pub(crate) fn post_raw<T: DeserializeOwned, P: Serialize>(
        &mut self,
        url: &str,
        params: &P,
    ) -> Result<T, ApiError> {
        self.check_expired()?;
        let req = ureq::post(url)
            .set("Accept", "application/json")
            .set("Authorization", &self.auth_header);

        let mut resp = req.clone().send_json(params)?;

        if resp.status() == 401 {
            self.refresh_token()?;
            resp = req.send_json(params)?
        }

        resp.into_json_with_error()
    }
}

/// Energy meter reading
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterReading {
    /// ID of the charger
    pub charger_id: String,

    /// Lifetime consumed energy, in kWh
    pub life_time_energy: f64,
}

impl Site {
    /// Read all energy meters from the given site
    pub fn lifetime_energy(&self, ctx: &mut Context) -> Result<Vec<MeterReading>, ApiError> {
        ctx.get(&format!("sites/{}/energy", self.id))
    }

    pub fn details(&self, ctx: &mut Context) -> Result<SiteDetails, ApiError> {
        ctx.get(&format!("sites/{}", self.id))
    }
}

impl Circuit {
    fn dynamic_current_path(&self) -> String {
        format!("sites/{}/circuits/{}/dynamicCurrent", self.site_id, self.id)
    }

    pub fn dynamic_current(&self, ctx: &mut Context) -> Result<Triphase, ApiError> {
        ctx.circuit_dynamic_current(self.site_id, self.id)
    }

    pub fn set_dynamic_current(
        &self,
        ctx: &mut Context,
        current: SetCurrent,
    ) -> Result<(), ApiError> {
        ctx.post(&self.dynamic_current_path(), &current)
    }
}

impl Charger {
    /// Enable "smart charging" on the charger. This just turns the LED blue, and disables basic charging plans.
    pub fn enable_smart_charging(&self, ctx: &mut Context) -> Result<(), ApiError> {
        let url = format!("chargers/{}/commands/smart_charging", &self.id);
        ctx.post(&url, &())
    }

    /// Read the state of a charger
    pub fn state(&self, ctx: &mut Context) -> Result<ChargerState, ApiError> {
        let url = format!("chargers/{}/state", self.id);
        ctx.get(&url)
    }

    /// Read info about the ongoing charging session
    pub fn ongoing_session(&self, ctx: &mut Context) -> Result<Option<ChargingSession>, ApiError> {
        ctx.maybe_get(&format!("chargers/{}/sessions/ongoing", &self.id))
    }

    /// Read info about the last charging session (not including ongoing one)
    pub fn latest_session(&self, ctx: &mut Context) -> Result<Option<ChargingSession>, ApiError> {
        ctx.maybe_get(&format!("chargers/{}/sessions/latest", &self.id))
    }

    fn command(&self, ctx: &mut Context, command: &str) -> Result<CommandReply, ApiError> {
        ctx.post(&format!("chargers/{}/commands/{}", self.id, command), &())
    }

    pub fn start(&self, ctx: &mut Context) -> Result<(), ApiError> {
        self.command(ctx, "start_charging")?;
        Ok(())
    }

    pub fn pause(&self, ctx: &mut Context) -> Result<(), ApiError> {
        self.command(ctx, "pause_charging")?;
        Ok(())
    }

    pub fn resume(&self, ctx: &mut Context) -> Result<(), ApiError> {
        self.command(ctx, "resume_charging")?;
        Ok(())
    }

    pub fn stop(&self, ctx: &mut Context) -> Result<(), ApiError> {
        self.command(ctx, "stop_charging")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::{Duration, Instant};

    use super::Context;
    #[test]
    fn token_save() {
        let ctx = Context {
            auth_header: "Bearer aaaaaaa0".to_owned(),
            refresh_token: "abcdef".to_owned(),
            token_expiration: Instant::now() + Duration::from_secs(1234),
            on_refresh: None,
        };

        let saved = ctx.save();
        let ctx2 = Context::from_saved(&saved).unwrap();

        assert_eq!(&ctx.auth_header, &ctx2.auth_header);
        assert_eq!(&ctx.refresh_token, &ctx2.refresh_token);
        assert!((ctx.token_expiration - ctx2.token_expiration) < Duration::from_secs(5))
    }
}
