use std::{cell::Cell, collections::HashMap, ffi::CString, time::Instant};

use windows::{
    Win32::{
        Foundation::{HANDLE, WAIT_OBJECT_0},
        System::{
            Memory::{FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS},
            Threading::{INFINITE, SYNCHRONIZATION_SYNCHRONIZE},
        },
    },
    core::PCSTR,
};

use crate::{
    error::IrpReaderError,
    snapshot::{Snapshot, TrackedVar, VarType},
    source::TelemetrySource,
};

const DATA_VALID_EVENT_NAME: &str = "Local\\IRSDKDataValidEvent";

const MEM_MAP_NAME: &str = "Local\\IRSDKMemMapFileName";

fn ir_str_to_string(str: &[u8]) -> String {
    let index = str.iter().position(|b| *b == 0).unwrap_or(str.len());
    String::from_utf8_lossy(&str[..index]).into_owned()
}

#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct irsdk_varBuf {
    pub tick_count: i32,
    pub buf_offset: i32,
    pub pad: [i32; 2],
}

const MAX_VAR_BUFS: usize = 4;
const MAX_STRING: usize = 32;
const MAX_DESC: usize = 64;

#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct irsdk_header {
    pub ver: i32,
    pub status: i32,
    pub tick_rate: i32,
    pub session_info_update: i32,
    pub session_info_len: i32,
    pub session_info_offset: i32,
    pub num_vars: i32,
    pub var_header_offset: i32,
    pub num_buf: i32,
    pub buf_len: i32,
    pub pad1: [i32; 2],
    pub var_buf: [irsdk_varBuf; MAX_VAR_BUFS],
}

#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct irsdk_varHeader {
    pub typ: i32,
    pub offset: i32,
    pub count: i32,
    pub count_as_time: bool,
    pub pad: [u8; 3],
    pub name: [u8; MAX_STRING],
    pub desc: [u8; MAX_DESC],
    pub unit: [u8; MAX_STRING],
}

#[derive(Debug)]
struct VarHeader {
    typ: VarType,
    offset: usize,
    count: usize,
    count_as_time: bool,
    name: String,
    desc: String,
    unit: String,
}

impl TryFrom<&irsdk_varHeader> for VarHeader {
    type Error = IrpReaderError;

    fn try_from(value: &irsdk_varHeader) -> Result<Self, Self::Error> {
        let typ = VarType::try_from(value.typ)?;
        Ok(VarHeader {
            typ,
            offset: value.offset as usize,
            count: value.count as usize,
            count_as_time: value.count_as_time,
            name: ir_str_to_string(&value.name),
            desc: ir_str_to_string(&value.desc),
            unit: ir_str_to_string(&value.unit),
        })
    }
}

pub struct MemMap {
    pub(crate) view: MEMORY_MAPPED_VIEW_ADDRESS,
    handle: HANDLE,
}

impl MemMap {
    pub fn open(name: &str) -> Result<Self, IrpReaderError> {
        let name = CString::new(name)?;
        let handle = unsafe {
            windows::Win32::System::Memory::OpenFileMappingA(
                FILE_MAP_READ.0,
                false,
                PCSTR::from_raw(name.as_ptr() as _),
            )
        }?;

        let view = unsafe {
            windows::Win32::System::Memory::MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0)
        };
        if view.Value.is_null() {
            return Err(IrpReaderError::MapViewFailed);
        }

        Ok(Self { handle, view })
    }

    pub(crate) unsafe fn as_ref<T>(&self, offset: usize) -> &T {
        unsafe { &*self.view.Value.byte_add(offset).cast::<T>() }
    }

    pub(crate) unsafe fn as_slice<T>(&self, offset: usize, len: usize) -> &[T] {
        unsafe { std::slice::from_raw_parts(self.view.Value.byte_add(offset).cast::<T>(), len) }
    }
}

impl Drop for MemMap {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Memory::UnmapViewOfFile(self.view).ok();
            windows::Win32::Foundation::CloseHandle(self.handle).ok();
        }
    }
}

pub struct Event {
    handle: HANDLE,
}

impl Event {
    pub fn open(name: &str) -> Result<Self, IrpReaderError> {
        let name = CString::new(name)?;
        let handle = unsafe {
            windows::Win32::System::Threading::OpenEventA(
                SYNCHRONIZATION_SYNCHRONIZE,
                false,
                PCSTR::from_raw(name.as_ptr() as _),
            )
        }?;
        Ok(Self { handle })
    }

    pub fn wait(&self) -> Result<Instant, IrpReaderError> {
        let result = unsafe {
            windows::Win32::System::Threading::WaitForSingleObject(self.handle, INFINITE)
        };
        if result != WAIT_OBJECT_0 {
            return Err(IrpReaderError::WaitFailed);
        }
        Ok(Instant::now())
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::Foundation::CloseHandle(self.handle).ok();
        }
    }
}

pub struct WindowsMmapSource {
    mem_map: MemMap,
    data_valid_event: Event,
    var_map: HashMap<String, VarHeader>,
    buf_len: usize,
    last_session_info_update: Cell<i32>,
}

impl WindowsMmapSource {
    pub fn connect() -> Result<Self, IrpReaderError> {
        let data_valid_event = Event::open(DATA_VALID_EVENT_NAME)?;
        let mem_map = MemMap::open(MEM_MAP_NAME)?;

        data_valid_event.wait()?;

        let header = unsafe { mem_map.as_ref::<irsdk_header>(0) };

        let var_headers = unsafe {
            mem_map.as_slice::<irsdk_varHeader>(
                header.var_header_offset as usize,
                header.num_vars as usize,
            )
        };

        let var_headers = var_headers
            .iter()
            .map(VarHeader::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let buf_len = header.buf_len as usize;

        let var_map = var_headers
            .into_iter()
            .map(|header| (header.name.to_string(), header))
            .collect();

        Ok(Self {
            mem_map,
            data_valid_event,
            var_map,
            buf_len,
            last_session_info_update: Cell::new(-1),
        })
    }
}

impl TelemetrySource for WindowsMmapSource {
    fn buf_len(&self) -> usize {
        self.buf_len
    }

    fn list_vars(&self) -> Vec<TrackedVar> {
        self.var_map
            .values()
            .map(|vh| TrackedVar {
                name: vh.name.clone(),
                typ: vh.typ,
                offset: vh.offset,
                count: vh.count,
            })
            .collect()
    }

    fn track(&self, names: &[&str]) -> Result<Vec<TrackedVar>, IrpReaderError> {
        names
            .iter()
            .map(|name| {
                let vh = self
                    .var_map
                    .get(*name)
                    .ok_or_else(|| IrpReaderError::UnknownVariable(name.to_string()))?;
                Ok(TrackedVar {
                    name: name.to_string(),
                    typ: vh.typ,
                    offset: vh.offset,
                    count: vh.count,
                })
            })
            .collect()
    }

    fn wait_for_snapshot(&self, max_retries: u32) -> Result<Option<Snapshot>, IrpReaderError> {
        let signaled_at = self.data_valid_event.wait()?;

        for _ in 0..max_retries + 1 {
            let header_ptr = self.mem_map.view.Value.cast::<irsdk_header>();
            let num_buf = unsafe { std::ptr::read_volatile(&(*header_ptr).num_buf) } as usize;
            let num_buf = num_buf.min(MAX_VAR_BUFS);

            let var_bufs = unsafe { &(&(*header_ptr).var_buf)[..num_buf] };
            let varbuf = var_bufs
                .iter()
                .max_by_key(|vb| unsafe { std::ptr::read_volatile(&vb.tick_count) })
                .ok_or(IrpReaderError::NoVarBufs)?;
            let tick_before = unsafe { std::ptr::read_volatile(&varbuf.tick_count) };
            let buf_offset = unsafe { std::ptr::read_volatile(&varbuf.buf_offset) } as usize;

            let snapshot = unsafe {
                self.mem_map
                    .as_slice::<u8>(buf_offset, self.buf_len)
                    .to_vec()
            };

            let tick_after = unsafe { std::ptr::read_volatile(&varbuf.tick_count) };

            if tick_before != tick_after {
                continue;
            }

            let last_session_info_update = self.last_session_info_update.get();
            let current_session_info_update_before =
                unsafe { std::ptr::read_volatile(&(*header_ptr).session_info_update) };
            let session_info = if last_session_info_update != current_session_info_update_before {
                let session_info_offset =
                    unsafe { std::ptr::read_volatile(&(*header_ptr).session_info_offset) } as usize;
                let session_info_len =
                    unsafe { std::ptr::read_volatile(&(*header_ptr).session_info_len) } as usize;

                Some(unsafe {
                    self.mem_map
                        .as_slice::<u8>(session_info_offset, session_info_len)
                        .to_vec()
                })
            } else {
                None
            };

            let current_session_info_update_after =
                unsafe { std::ptr::read_volatile(&(*header_ptr).session_info_update) };
            if current_session_info_update_before != current_session_info_update_after {
                continue;
            }

            self.last_session_info_update
                .set(current_session_info_update_before);

            return Ok(Some(Snapshot::new(
                tick_before,
                snapshot,
                signaled_at,
                session_info,
            )));
        }

        Err(IrpReaderError::InconsistentFrame)
    }
}
