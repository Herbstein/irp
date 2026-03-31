use std::time::Instant;

use crate::error::IrpReaderError;

#[derive(Debug, Copy, Clone)]
#[repr(i32)]
pub enum VarType {
    Char = 0,
    Bool = 1,
    Int = 2,
    Bitfield = 3,
    Float = 4,
    Double = 5,
}

impl VarType {
    pub(crate) fn byte_size(&self) -> usize {
        match self {
            VarType::Char | VarType::Bool => 1,
            VarType::Int | VarType::Bitfield | VarType::Float => 4,
            VarType::Double => 8,
        }
    }
}

impl TryFrom<i32> for VarType {
    type Error = IrpReaderError;

    fn try_from(i: i32) -> Result<Self, Self::Error> {
        match i {
            0 => Ok(VarType::Char),
            1 => Ok(VarType::Bool),
            2 => Ok(VarType::Int),
            3 => Ok(VarType::Bitfield),
            4 => Ok(VarType::Float),
            5 => Ok(VarType::Double),
            _ => Err(IrpReaderError::InvalidVarType(i)),
        }
    }
}

#[derive(Debug)]
pub struct TrackedVar {
    pub name: String,
    pub typ: VarType,
    pub offset: usize,
    pub count: usize,
}

pub struct Snapshot {
    tick_count: i32,
    buf: Vec<u8>,
    signaled_at: Instant,
    session_info: Option<Vec<u8>>,
}

impl Snapshot {
    pub fn new(
        tick_count: i32,
        buf: Vec<u8>,
        signaled_at: Instant,
        session_info: Option<Vec<u8>>,
    ) -> Self {
        Self {
            tick_count,
            buf,
            signaled_at,
            session_info,
        }
    }

    pub fn tick_count(&self) -> i32 {
        self.tick_count
    }

    pub fn buf(&self) -> &[u8] {
        &self.buf
    }

    pub fn signaled_at(&self) -> Instant {
        self.signaled_at
    }

    pub fn session_info(&self) -> Option<&[u8]> {
        self.session_info.as_deref()
    }
}
