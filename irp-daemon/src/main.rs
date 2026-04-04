use std::sync::{Arc, Mutex};

use irp_reader::{FromTelemetrySnapshot, IrpReaderError, SnapshotReader};
use serde::Deserialize;
use tokio::sync::Notify;

mod connection;
mod sim_reader;

#[derive(Clone, Debug)]
pub struct LiveData {
    is_on_track: bool,
    car_idx_lap_dist_pct: Vec<f32>,
    fuel_level: f32,
    fuel_level_pct: f32,
}

impl FromTelemetrySnapshot for LiveData {
    const REQUIRED_VARS: &[&str] = &["CarIdxLapDistPct", "IsOnTrack", "FuelLevel", "FuelLevelPct"];

    fn from_snapshot(reader: &SnapshotReader) -> Result<Self, IrpReaderError> {
        Ok(LiveData {
            is_on_track: reader.get_bool("IsOnTrack")?,
            car_idx_lap_dist_pct: reader.get_float_array("CarIdxLapDistPct")?,
            fuel_level: reader.get_float("FuelLevel")?,
            fuel_level_pct: reader.get_float("FuelLevelPct")?,
        })
    }
}

#[derive(Clone, Deserialize)]
pub struct WeekendInfo {
    #[serde(rename = "TrackID")]
    track_id: i32,
    #[serde(rename = "SubSessionID")]
    sub_session_id: i32,
}

#[derive(Clone, Deserialize)]
pub struct Driver {
    #[serde(rename = "CarIdx")]
    car_idx: i32,
    #[serde(rename = "UserID")]
    user_id: i32,
    #[serde(rename = "UserName")]
    user_name: String,
}

#[derive(Clone, Deserialize)]
pub struct DriverInfo {
    #[serde(rename = "DriverCarIdx")]
    driver_car_idx: i32,
    #[serde(rename = "Drivers")]
    drivers: Vec<Driver>,
}

#[derive(Clone, Deserialize)]
pub struct SessionInfo {
    #[serde(rename = "WeekendInfo")]
    weekend_info: WeekendInfo,
    #[serde(rename = "DriverInfo")]
    driver_info: DriverInfo,
}

#[derive(Clone)]
enum SessionState {
    New(SessionInfo),
    Existing(SessionInfo),
    None,
}

impl SessionState {
    pub fn is_new(&self) -> bool {
        matches!(self, SessionState::New(_))
    }

    pub fn session_info(&self) -> Option<&SessionInfo> {
        match self {
            SessionState::Existing(session_info) => Some(session_info),
            _ => None,
        }
    }

    pub fn take_session_info(&mut self) -> Option<SessionInfo> {
        match std::mem::replace(self, SessionState::None) {
            SessionState::New(session_info) | SessionState::Existing(session_info) => {
                Some(session_info)
            }
            _ => None,
        }
    }
}

#[derive(Clone)]
struct DaemonState {
    session_info: SessionState,
    latest_telemetry: Option<LiveData>,
    sim_connected: bool,
}

struct Shared {
    state: Mutex<DaemonState>,
    notify: Notify,
}

type State = Arc<Shared>;

#[tokio::main]
async fn main() {
    let state = Arc::new(Shared {
        state: Mutex::new(DaemonState {
            session_info: SessionState::None,
            latest_telemetry: None,
            sim_connected: false,
        }),
        notify: Notify::new(),
    });

    sim_reader::sim_reader(state.clone());

    connection::connect(state).await;
}
