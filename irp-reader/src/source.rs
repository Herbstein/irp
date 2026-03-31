use crate::{
    snapshot::{Snapshot, TrackedVar},
    error::IrpReaderError,
};

pub trait TelemetrySource {
    fn buf_len(&self) -> usize;

    fn list_vars(&self) -> Vec<TrackedVar>;

    fn track(&self, names: &[&str]) -> Result<Vec<TrackedVar>, IrpReaderError>;

    fn wait_for_snapshot(&self, max_retries: u32) -> Result<Option<Snapshot>, IrpReaderError>;
}
