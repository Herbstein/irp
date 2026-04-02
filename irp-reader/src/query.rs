use std::marker::PhantomData;

use crate::{
    error::IrpReaderError,
    reader::SnapshotReader,
    snapshot::{Snapshot, TrackedVar},
    source::TelemetrySource,
};

pub trait FromTelemetrySnapshot: Sized {
    const REQUIRED_VARS: &[&str];

    fn from_snapshot(reader: &SnapshotReader) -> Result<Self, IrpReaderError>;
}

pub struct TelemetryQuery<T: FromTelemetrySnapshot> {
    tracked: Vec<TrackedVar>,
    _marker: PhantomData<T>,
}

impl<T> TelemetryQuery<T>
where
    T: FromTelemetrySnapshot,
{
    pub fn new<S: TelemetrySource>(source: &S) -> Result<Self, IrpReaderError> {
        let tracked = source.track(T::REQUIRED_VARS)?;
        Ok(Self {
            tracked,
            _marker: PhantomData,
        })
    }

    pub fn deserialize(&self, snapshot: &Snapshot) -> Result<T, IrpReaderError> {
        let reader = SnapshotReader::new(snapshot, &self.tracked);
        T::from_snapshot(&reader)
    }
}
