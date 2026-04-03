use irp_reader::{
    FromTelemetrySnapshot, IrpReaderError, SnapshotReader, TelemetryQuery, TelemetrySource,
    WindowsMmapSource,
};

#[derive(Debug)]
struct CarTelemetry {
    speed: f32,
    rpm: f32,
    gear: i32,
}

impl FromTelemetrySnapshot for CarTelemetry {
    const REQUIRED_VARS: &[&str] = &["Speed", "RPM", "Gear"];

    fn from_snapshot(reader: &SnapshotReader) -> Result<Self, IrpReaderError> {
        Ok(Self {
            speed: reader.get_float("Speed")?,
            rpm: reader.get_float("RPM")?,
            gear: reader.get_int("Gear")?,
        })
    }
}

fn main() -> Result<(), IrpReaderError> {
    let mut source = WindowsMmapSource::connect()?;

    let telemetry = TelemetryQuery::<CarTelemetry>::new(&source)?;

    while let Some(snapshot) = source.wait_for_snapshot()? {
        let telemetry = telemetry.deserialize(&snapshot)?;

        let elapsed = snapshot.signaled_at().elapsed();

        println!(
            "Tick {}: Speed={:.2} RPM={:.0} Gear={} (elapsed: {:?})",
            snapshot.tick_count(),
            telemetry.speed,
            telemetry.rpm,
            telemetry.gear,
            elapsed
        );
    }

    Ok(())
}
