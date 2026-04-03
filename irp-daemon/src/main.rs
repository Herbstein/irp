use std::sync::{Arc, Mutex};

use irp_reader::{FromTelemetrySnapshot, IrpReaderError, SnapshotReader};
use serde::Deserialize;
use tokio::sync::{Notify, watch};

mod connection;
mod sim_reader;

#[derive(Clone, Debug)]
pub struct LiveData {
    is_on_track: bool,
    car_idx_lap_dist_pct: Vec<f32>,
}

impl FromTelemetrySnapshot for LiveData {
    const REQUIRED_VARS: &[&str] = &["CarIdxLapDistPct", "IsOnTrack"];

    fn from_snapshot(reader: &SnapshotReader) -> Result<Self, IrpReaderError> {
        Ok(LiveData {
            is_on_track: reader.get_bool("IsOnTrack")?,
            car_idx_lap_dist_pct: reader.get_float_array("CarIdxLapDistPct")?,
        })
    }
}

#[derive(Clone, Deserialize)]
pub struct WeekendInfo {
    #[serde(rename = "SubSessionID")]
    sub_session_id: i32,
}

#[derive(Clone, Deserialize)]
pub struct Driver {
    #[serde(rename = "CarIdx")]
    car_idx: i32,
    #[serde(rename = "UserID")]
    user_id: i32,
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
pub struct LapSummary {}

#[derive(Clone)]
struct DaemonState {
    latest_session_info: Option<SessionInfo>,
    pending_lap_summaries: Vec<LapSummary>,
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
            latest_session_info: None,
            pending_lap_summaries: Vec::new(),
            latest_telemetry: None,
            sim_connected: false,
        }),
        notify: Notify::new(),
    });

    sim_reader::sim_reader(state.clone());

    connection::connect(state).await;
}
