use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Write};

use navo_core::{FsError, FsErrorKind, Location};

use super::{error, io_error};
const CHUNK: usize = 1 << 20;

thread_local! {
    static BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    #[cfg(test)]
    static ALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) enum PumpError {
    Read(std::io::Error),
    Write(std::io::Error),
    Cancelled,
}

impl PumpError {
    pub(super) fn located(self, source: &Location, target: &Location) -> FsError {
        match self {
            Self::Read(e)
                if e.get_ref()
                    .is_some_and(|error| error.is::<crate::git::FilterReadError>()) =>
            {
                error(FsErrorKind::SourceFilterFailed, source)
            }
            Self::Read(e) => match e
                .get_ref()
                .and_then(|e| e.downcast_ref::<navo_git::GitError>())
            {
                Some(navo_git::GitError::Timeout) => error(FsErrorKind::Timeout, source),
                Some(navo_git::GitError::Command { .. }) => {
                    error(FsErrorKind::SourceFilterFailed, source)
                }
                _ => io_error(&e, source),
            },
            Self::Write(e) => io_error(&e, target),
            Self::Cancelled => error(FsErrorKind::Cancelled, target),
        }
    }
}

pub(super) fn pump<R: Read + ?Sized>(
    input: &mut R,
    output: &mut File,
    control: &mut crate::CopyControl<'_>,
) -> Result<u64, PumpError> {
    let mut buffer = BUFFER.with_borrow_mut(std::mem::take);
    if buffer.len() < CHUNK {
        buffer.resize(CHUNK, 0);
        #[cfg(test)]
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
    }
    let result = pump_into(input, output, control, &mut buffer);
    BUFFER.with_borrow_mut(|slot| *slot = buffer);
    result
}

fn pump_into<R: Read + ?Sized>(
    input: &mut R,
    output: &mut File,
    control: &mut crate::CopyControl<'_>,
    buffer: &mut [u8],
) -> Result<u64, PumpError> {
    let mut total = 0_u64;
    loop {
        if control.cancel.is_cancelled() {
            return Err(PumpError::Cancelled);
        }
        let read = match input.read(buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(PumpError::Read)?,
        };
        if read == 0 {
            return Ok(total);
        }
        let chunk = buffer.get(..read).unwrap_or_default();
        output.write_all(chunk).map_err(PumpError::Write)?;
        total += read as u64;
        (control.progress)(total);
    }
}

#[cfg(test)]
pub(super) fn allocations() -> usize {
    ALLOCATIONS.with(std::cell::Cell::get)
}
