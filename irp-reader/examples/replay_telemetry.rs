use irp_reader::{
    FileReplaySource, FromTelemetrySnapshot, IrpReaderError, SnapshotReader, TelemetryQuery,
    TelemetrySource,
};

#[derive(Debug)]
struct RacePositions {
    lap_dist_pct: f32,
    car_idx_lap_dist_pct: Vec<f32>,
}

impl FromTelemetrySnapshot for RacePositions {
    const REQUIRED_VARS: &[&str] = &["LapDistPct", "CarIdxLapDistPct"];

    fn from_snapshot(reader: &SnapshotReader) -> Result<Self, IrpReaderError> {
        Ok(Self {
            lap_dist_pct: reader.get_float("LapDistPct")?,
            car_idx_lap_dist_pct: reader.get_float_array("CarIdxLapDistPct")?,
        })
    }
}

fn main() -> Result<(), IrpReaderError> {
    let source = FileReplaySource::open("session.irp", true)?;

    let positions = TelemetryQuery::<RacePositions>::new(&source)?;

    while let Some(snapshot) = source.wait_for_snapshot(0)? {
        let positions = positions.deserialize(&snapshot)?;

        let elapsed = snapshot.signaled_at().elapsed();

        println!(
            "Tick {}: LapDist={:.2} (elapsed: {:?})",
            snapshot.tick_count(),
            positions.lap_dist_pct,
            elapsed
        );
    }

    Ok(())
}
