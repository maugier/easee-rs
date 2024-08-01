use std::{io, time::{Duration, Instant}};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, instrument};

#[derive(Debug)]
pub struct Context {
    auth_header: String,
    refresh_token: String,
    token_expiration: Instant,
}

const API_BASE: &'static str = "https://api.easee.com/api/";
const REFRESH_TOKEN_DELAY: Duration = Duration::from_secs(600);

fn parse_iso8601(s: &str) -> Result<NaiveDateTime, ApiError> {
    Ok(NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")?)
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct Charger {
    pub id: String,
    pub name: String,
    pub product_code: u32,
    pub color: Option<i32>,
    pub created_on: DateTime<Utc>,
    pub updated_on: DateTime<Utc>,
    pub level_of_access: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct ChargerState {
    pub smart_charging: bool,
    pub cable_locked: bool,
    pub charger_op_mode: u32,
    pub total_power: f64,
    pub session_energy: f64,
    pub energy_per_hour: f64,

    #[serde(rename="wiFiRSSI")]
    pub wifi_rssi: u32,

    #[serde(rename="cellRSSI")]
    pub cell_rssi: u32,

    #[serde(rename="localRSSI")]
    pub local_rssi: u32,
    pub output_phase: u32,
    pub dynamic_circuit_current_p1: u32,   
    pub dynamic_circuit_current_p2: u32,
    pub dynamic_circuit_current_p3: u32,
    pub latest_pulse: DateTime<Utc>,
    pub charger_firmware: u32,
    pub voltage: f64,

    #[serde(rename="chargerRAT")]
    pub charger_rat: u32,
    pub lock_cable_permanently: bool,
    pub in_current_t2: f64,
    pub in_current_t3: f64,
    pub in_current_t4: f64,
    pub in_current_t5: f64,
    pub output_current: f64,
    pub is_online: bool,
    pub in_voltage_t1_t2: f64,
    pub in_voltage_t1_t3: f64,
    pub in_voltage_t1_t4: f64,
    pub in_voltage_t1_t5: f64,
    pub in_voltage_t2_t3: f64,
    pub in_voltage_t2_t4: f64,
    pub in_voltage_t2_t5: f64,
    pub in_voltage_t3_t4: f64,
    pub in_voltage_t3_t5: f64,
    pub in_voltage_t4_t5: f64,
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

    #[serde(rename="WifiAPEnabled")]
    pub wifi_ap_enabled: bool,
    pub lifetime_energy: f64,
    pub offline_max_circuit_current_p1: u32,
    pub offline_max_circuit_current_p2: u32,
    pub offline_max_circuit_current_p3: u32,
    pub error_code: u32,
    pub fault_error_code: u32,
    pub eq_available_current_p1: f64,
    pub eq_available_current_p2: f64,
    pub eq_available_current_p3: f64,
    pub derated_current: f64,
    pub derating_active: bool,
    pub connected_to_cloud: bool,

}

#[derive(Debug,Deserialize)]
pub struct Address {

}

#[derive(Debug,Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub uuid: Option<String>,
    pub id: u32,
    pub site_key: Option<String>,
    pub name: Option<String>,
    pub level_of_access: u32,
    //pub address: Address,
    pub installer_alias: Option<String>
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub access_token: String,
    pub expires_in: u32,
    pub access_claims: Vec<Option<String>>,
    pub token_type: Option<String>,
    pub refresh_token: String
}

#[derive(Debug,Error)]
pub enum ApiError {
    #[error("io: {0}")]
    IO(#[from] io::Error),

    #[error("ureq")]
    Ureq(#[from] ureq::Error),

    #[error("unexpected data: {1} when processing {0}")]
    UnexpectedData(serde_json::Value, serde_json::Error),

    #[error("format error: {0}")]
    FormatError(#[from] chrono::ParseError)
}

trait JsonExplicitError {
    fn into_json_with_error<T: DeserializeOwned>(self) -> Result<T, ApiError>;
}

impl JsonExplicitError for ureq::Response {
    fn into_json_with_error<T: DeserializeOwned>(self) -> Result<T, ApiError> {
        let resp: serde_json::Value = self.into_json()?;
        let parsed = T::deserialize(&resp);
        parsed.map_err(|e| ApiError::UnexpectedData(resp, e))        
    }
}

impl Context {

    pub fn from_tokens(access_token: &str, refresh_token: String, expires_in: u32) -> Self {   
        Self { auth_header: format!("Bearer {}", access_token),
               refresh_token,
               token_expiration: Instant::now() + Duration::from_secs(expires_in as u64) - REFRESH_TOKEN_DELAY }
    }

    fn from_login_response(resp: LoginResponse) -> Self {
        Self::from_tokens(&resp.access_token, resp.refresh_token, resp.expires_in)
    }

    pub fn from_login(user: &str, password: &str) -> Result<Self, ApiError> {
        #[derive(Serialize)]
        #[serde(rename_all="camelCase")]
        struct Params<'t> { user_name: &'t str, password: &'t str }

        info!("Logging into API");
        let url: String = format!("{}accounts/login", API_BASE);
        let resp: LoginResponse = ureq::post(&url)
            .send_json(Params { user_name: user, password } )?
            .into_json_with_error()?;

        Ok(Self::from_login_response(resp))
    }

    fn check_expired(&mut self) -> Result<(), ApiError> {
        if self.token_expiration < Instant::now() {
            debug!("Token has expired");
            self.refresh_token()?;
        }
        Ok(())
    }

    pub fn refresh_token(&mut self) -> Result<(), ApiError> {
        #[derive(Serialize)]
        #[serde(rename_all="camelCase")]
        struct Params<'t> { refresh_token: &'t str }

        info!("Refreshing access token");
        let params = Params { refresh_token: &self.refresh_token };
        let url = format!("{}accounts/refresh_token", API_BASE);
        let resp: LoginResponse = ureq::post(&url)
            .set("Content-type", "application/json")
            .send_json(&params)?
            .into_json_with_error()?;

        *self = Self::from_login_response(resp);
        Ok(())

    }

    pub fn sites(&mut self) -> Result<Vec<Site>, ApiError> {
        self.get("sites")
    }

    pub fn chargers(&mut self) -> Result<Vec<Charger>, ApiError> {
        self.get("chargers")
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
        
        Ok(resp.into_json_with_error()?)
    }  

    fn post<T: DeserializeOwned, P: Serialize>(&mut self, path: &str, params: &P) -> Result<T, ApiError> {
        self.check_expired()?;
        let url: String = format!("{}{}", API_BASE, path);
        let req = ureq::post(&url)
            .set("Accept", "application/json")
            .set("Authorization", &self.auth_header);

        let mut resp = req.clone().send_json(params)?;

        if resp.status() == 401 {
            self.refresh_token()?;
            resp = req.send_json(params)?
        }
        
        Ok(resp.into_json_with_error()?)     
    }

}

#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct MeterReading {
    pub charger_id: String,
    pub life_time_energy: f64,
}

impl Site {
    pub fn lifetime_energy(&self, ctx: &mut Context) -> Result<Vec<MeterReading>, ApiError> {
        ctx.get(&format!("sites/{}/energy", self.id))
    }
}

impl Charger {
    pub fn enable_smart_charging(&self, ctx: &mut Context) -> Result<(), ApiError> {
        let url = format!("chargers/{}/commands/smart_charging", &self.id);
        ctx.post(&url, &())       
    }

    pub fn state(&self, ctx: &mut Context) -> Result<ChargerState, ApiError> {
        let url = format!("chargers/{}/state", self.id);
        ctx.get(&url)
    }
}