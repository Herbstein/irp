use irp_reader::{IrpReaderError, TelemetryRecorder, TelemetrySource, WindowsMmapSource};

fn main() -> Result<(), IrpReaderError> {
    let source = WindowsMmapSource::connect()?;

    let mut recorder = TelemetryRecorder::create("session.irp", &source)?;

    while let Some(snapshot) = source.wait_for_snapshot()? {
        recorder.record(&snapshot)?;

        let elapsed = snapshot.signaled_at().elapsed();

        println!("Tick {} (elapsed: {:?})", snapshot.tick_count(), elapsed);
    }

    Ok(())
}
