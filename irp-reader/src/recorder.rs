use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Instant,
};

use crate::{
    error::IrpReaderError,
    snapshot::{Snapshot, TrackedVar},
    source::TelemetrySource,
};

pub struct TelemetryRecorder {
    writer: BufWriter<File>,
    last_snapshot_at: Option<Instant>,
}

impl TelemetryRecorder {
    pub fn create<S: TelemetrySource>(path: &str, source: &S) -> Result<Self, IrpReaderError> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        let all_vars = source.list_vars();
        let buf_len = source.buf_len() as u32;
        Self::write_header(&mut writer, &all_vars, buf_len)?;

        Ok(Self {
            writer,
            last_snapshot_at: None,
        })
    }

    pub fn record(&mut self, snapshot: &Snapshot) -> Result<(), IrpReaderError> {
        let delay_micros = match self.last_snapshot_at {
            Some(prev) => snapshot.signaled_at().duration_since(prev).as_micros() as u64,
            None => 0,
        };

        self.last_snapshot_at = Some(snapshot.signaled_at());

        self.writer.write_all(&delay_micros.to_le_bytes())?;
        self.writer
            .write_all(&snapshot.tick_count().to_le_bytes())?;
        self.writer.write_all(snapshot.buf())?;

        self.writer.flush()?;

        Ok(())
    }

    fn write_header(
        writer: &mut BufWriter<File>,
        vars: &[TrackedVar],
        buf_len: u32,
    ) -> Result<(), IrpReaderError> {
        writer.write_all(&(vars.len() as u32).to_le_bytes())?;
        writer.write_all(&buf_len.to_le_bytes())?;

        for var in vars {
            let name_bytes = var.name.as_bytes();
            writer.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(name_bytes)?;
            writer.write_all(&(var.typ as i32).to_le_bytes())?;
            writer.write_all(&(var.offset as u32).to_le_bytes())?;
            writer.write_all(&(var.count as u32).to_le_bytes())?;
        }

        writer.flush()?;

        Ok(())
    }
}

impl Drop for TelemetryRecorder {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}
