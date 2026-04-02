use thiserror::Error;

use crate::snapshot::VarType;

#[derive(Debug, Error)]
pub enum IrpReaderError {
    #[error("Invalid VarType discriminant: {0}")]
    InvalidVarType(i32),
    #[error("Unknown telemetry variable: {0}")]
    UnknownVariable(String),
    #[error("Variable '{name}' is not a scalar (count = {count}), use get_*_array instead")]
    NotAScalar { name: String, count: usize },
    #[error("Variable '{name}' has type '{actual:?}', expected '{expected}'")]
    TypeMismatch {
        name: String,
        actual: VarType,
        expected: &'static str,
    },
    #[error("Variable '{0}' out of bounds")]
    OutOfBounds(String),
    #[error("Failed to map view of file")]
    MapViewFailed,
    #[error("No varbufs available")]
    NoVarBufs,
    #[error("Failed to get a consistent frame after retries")]
    InconsistentFrame,
    #[error("Event wait failed")]
    WaitFailed,
    #[error("Replay ended")]
    EndOfReplay,
    #[error("iRacing memory map not found")]
    MemoryMapMissing,

    #[error(transparent)]
    #[cfg(feature = "windows-mmap")]
    Windows(#[from] windows::core::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Nul(#[from] std::ffi::NulError),
}
