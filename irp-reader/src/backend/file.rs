use std::{
    cell::Cell,
    collections::HashMap,
    fs::File,
    io,
    io::{BufReader, Read},
    thread,
    time::{Duration, Instant},
};

use crate::{
    error::IrpReaderError,
    snapshot::{Snapshot, TrackedVar, VarType},
    source::TelemetrySource,
};

#[derive(Debug)]
struct VarDescriptor {
    name: String,
    typ: VarType,
    offset: usize,
    count: usize,
}

struct RecordedFrame {
    delay: Duration,
    tick_count: i32,
    buf: Vec<u8>,
}

pub struct FileReplaySource {
    vars: HashMap<String, VarDescriptor>,
    frames: Vec<RecordedFrame>,
    cursor: Cell<usize>,
    realtime: bool,
    buf_len: usize,
}

impl FileReplaySource {
    pub fn open(path: &str, realtime: bool) -> Result<Self, IrpReaderError> {
        let file = File::open(path)?;
        let mut file = BufReader::new(file);

        let num_vars = read_u32(&mut file)? as usize;
        let buf_len = read_u32(&mut file)? as usize;

        let mut vars = HashMap::with_capacity(num_vars);
        for _ in 0..num_vars {
            let name_len = read_u32(&mut file)? as usize;
            let mut name_buf = vec![0; name_len];
            file.read_exact(&mut name_buf).map_err(IrpReaderError::Io)?;
            let name = String::from_utf8(name_buf)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            let type_id = read_i32(&mut file)?;
            let typ = VarType::try_from(type_id)?;
            let offset = read_u32(&mut file)? as usize;
            let count = read_u32(&mut file)? as usize;

            vars.insert(
                name.clone(),
                VarDescriptor {
                    name,
                    typ,
                    offset,
                    count,
                },
            );
        }

        let mut frames = Vec::new();
        loop {
            let mut delay_buf = [0; 8];
            match file.read_exact(&mut delay_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let delay_micros = u64::from_le_bytes(delay_buf);

            let tick_count = read_i32(&mut file)?;

            let mut buf = vec![0; buf_len];
            file.read_exact(&mut buf)?;

            frames.push(RecordedFrame {
                delay: Duration::from_micros(delay_micros),
                tick_count,
                buf,
            });
        }

        Ok(Self {
            vars,
            frames,
            cursor: Cell::new(0),
            realtime,
            buf_len,
        })
    }
}

impl TelemetrySource for FileReplaySource {
    fn buf_len(&self) -> usize {
        self.buf_len
    }

    fn list_vars(&self) -> Vec<TrackedVar> {
        self.vars
            .values()
            .map(|vd| TrackedVar {
                name: vd.name.clone(),
                typ: vd.typ,
                offset: vd.offset,
                count: vd.count,
            })
            .collect()
    }

    fn track(&self, names: &[&str]) -> Result<Vec<TrackedVar>, IrpReaderError> {
        names
            .iter()
            .map(|name| {
                let vh = self
                    .vars
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

    fn wait_for_snapshot(&self, _max_retries: u32) -> Result<Option<Snapshot>, IrpReaderError> {
        let idx = self.cursor.get();
        if idx >= self.frames.len() {
            return Ok(None);
        }

        let frame = &self.frames[idx];
        self.cursor.set(idx + 1);

        if self.realtime && idx > 0 {
            thread::sleep(frame.delay);
        }

        Ok(Some(Snapshot::new(
            frame.tick_count,
            frame.buf.clone(),
            Instant::now(),
        )))
    }
}

fn read_u32(r: &mut impl Read) -> Result<u32, IrpReaderError> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i32(r: &mut impl Read) -> Result<i32, IrpReaderError> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}
