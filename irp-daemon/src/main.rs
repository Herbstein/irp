use irp_reader::{FromTelemetrySnapshot, IrpReaderError, SnapshotReader};

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

#[derive(Clone, Debug)]
pub enum IrpMessage {
    Telemetry(LiveData),
    LapSummary,
    SessionInfo,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    sim_reader::sim_reader(tx);

    connection::connect(rx).await;
}
