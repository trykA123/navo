use super::{error, io_error};
use rusty_core::{FsError, FsErrorKind, Location};
use std::fs::File;
use std::io::{Read, Write};
const CHUNK: usize = 1 << 20;

pub(super) enum PumpError {
    Read(std::io::Error),
    Write(std::io::Error),
    Cancelled,
}

impl PumpError {
    pub(super) fn located(self, source: &Location, target: &Location) -> FsError {
        match self {
            Self::Read(e) => io_error(&e, source),
            Self::Write(e) => io_error(&e, target),
            Self::Cancelled => error(FsErrorKind::Cancelled, target),
        }
    }
}

pub(super) fn pump(
    input: &mut File,
    output: &mut File,
    control: &mut crate::CopyControl<'_>,
) -> Result<u64, PumpError> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut total = 0_u64;
    loop {
        if control.cancel.is_cancelled() {
            return Err(PumpError::Cancelled);
        }
        let read = input.read(&mut buffer).map_err(PumpError::Read)?;
        if read == 0 {
            return Ok(total);
        }
        let chunk = buffer.get(..read).unwrap_or_default();
        output.write_all(chunk).map_err(PumpError::Write)?;
        total += read as u64;
        (control.progress)(total);
    }
}
