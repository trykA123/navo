#![allow(unsafe_code, reason = "std has no safe wrapper for CopyFile2")]

use std::any::Any;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::Path;

use rusty_core::{FsError, FsErrorKind, Location};
use windows_sys::Win32::Foundation::{
    ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED, ERROR_REQUEST_ABORTED,
    HANDLE,
};
use windows_sys::Win32::Storage::FileSystem::{
    COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, COPYFILE2_CALLBACK_CHUNK_FINISHED,
    COPYFILE2_CALLBACK_STREAM_FINISHED, COPYFILE2_EXTENDED_PARAMETERS, COPYFILE2_MESSAGE,
    COPYFILE2_MESSAGE_ACTION, COPYFILE2_PROGRESS_CANCEL, COPYFILE2_PROGRESS_CONTINUE, CopyFile2,
    FlushFileBuffers,
};

use super::{AutoTuning, error, io_error, os_path};
use crate::CancelToken;

pub(super) type Flush = fn(HANDLE) -> io::Result<()>;

pub(super) struct CopyProgress<'a> {
    report: &'a mut dyn FnMut(u64),
    cancel: &'a CancelToken,
    total: u64,
    flush: Flush,
    flushed: bool,
    flush_error: Option<io::Error>,
    panic: Option<Box<dyn Any + Send>>,
}

impl<'a> CopyProgress<'a> {
    pub(super) fn new(report: &'a mut dyn FnMut(u64), cancel: &'a CancelToken) -> Self {
        Self {
            report,
            cancel,
            total: 0,
            flush: flush_handle,
            flushed: false,
            flush_error: None,
            panic: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_flush(self, flush: Flush) -> Self {
        Self { flush, ..self }
    }

    fn on_message(&mut self, message: &COPYFILE2_MESSAGE) -> bool {
        let (stream, bytes) = match message.Type {
            COPYFILE2_CALLBACK_CHUNK_FINISHED => {
                // SAFETY: Type selects the initialized ChunkFinished union member.
                let info = unsafe { message.Info.ChunkFinished };
                (info.dwStreamNumber, info.uliStreamBytesTransferred)
            }
            COPYFILE2_CALLBACK_STREAM_FINISHED => {
                // SAFETY: Type selects the initialized StreamFinished union member.
                let info = unsafe { message.Info.StreamFinished };
                if info.dwStreamNumber == 1
                    && let Err(failure) = (self.flush)(info.hDestinationFile)
                {
                    self.flush_error = Some(failure);
                    return false;
                }
                self.flushed |= info.dwStreamNumber == 1;
                (info.dwStreamNumber, info.uliStreamBytesTransferred)
            }
            _ => return true,
        };
        if stream == 1 {
            self.total = self.total.max(bytes);
            (self.report)(self.total);
        }
        true
    }
}

fn flush_handle(handle: HANDLE) -> io::Result<()> {
    // SAFETY: CopyFile2 keeps the destination handle open for the duration of the callback.
    if unsafe { FlushFileBuffers(handle) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct Paths<'a> {
    source: &'a Location,
    target: &'a Location,
    source_wide: Vec<u16>,
    target_wide: Vec<u16>,
}

pub(super) fn copy(
    source: &Location,
    target: &Location,
    tuning: &AutoTuning,
    mut progress: CopyProgress<'_>,
) -> Result<u64, FsError> {
    if progress.cancel.is_cancelled() {
        return Err(error(FsErrorKind::Cancelled, target));
    }
    let source_path = os_path(source)?;
    let target_path = os_path(target)?;
    let size = fs::metadata(&source_path)
        .map_err(|e| io_error(&e, source))?
        .len();
    let paths = Paths {
        source,
        target,
        source_wide: wide(&source_path),
        target_wide: wide(&target_path),
    };
    let mut flags = COPY_FILE_FAIL_IF_EXISTS;
    if size >= tuning.unbuffered_from {
        flags |= COPY_FILE_NO_BUFFERING;
    }
    let mut result = attempt(&paths, flags, &mut progress)?;
    if result < 0 && retries_buffered(flags, win32_code(result)) {
        remove_part(&target_path, target)?;
        result = attempt(&paths, flags & !COPY_FILE_NO_BUFFERING, &mut progress)?;
    }
    if let Some(failure) = progress.flush_error.take() {
        return Err(io_error(&failure, target));
    }
    if result < 0 {
        return Err(copy_error(win32_code(result), &source_path, &paths));
    }
    if progress.cancel.is_cancelled() {
        return Err(error(FsErrorKind::Cancelled, target));
    }
    if progress.total > 0 && !progress.flushed {
        return Err(error(FsErrorKind::Unknown, target));
    }
    Ok(progress.total)
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn attempt(paths: &Paths<'_>, flags: u32, progress: &mut CopyProgress<'_>) -> Result<i32, FsError> {
    progress.flushed = false;
    progress.flush_error = None;
    let parameters = COPYFILE2_EXTENDED_PARAMETERS {
        dwSize: u32::try_from(size_of::<COPYFILE2_EXTENDED_PARAMETERS>())
            .map_err(|_| error(FsErrorKind::Unknown, paths.target))?,
        dwCopyFlags: flags,
        pfCancel: std::ptr::null_mut(),
        pProgressRoutine: Some(on_progress),
        pvCallbackContext: std::ptr::from_mut(progress).cast(),
    };
    // SAFETY: paths are NUL-terminated; parameters and exclusive callback state outlive this synchronous call.
    let result = unsafe {
        CopyFile2(
            paths.source_wide.as_ptr(),
            paths.target_wide.as_ptr(),
            &parameters,
        )
    };
    if let Some(payload) = progress.panic.take() {
        resume_unwind(payload);
    }
    Ok(result)
}

fn remove_part(path: &Path, target: &Location) -> Result<(), FsError> {
    match fs::remove_file(path) {
        Err(failure) if failure.kind() != io::ErrorKind::NotFound => {
            Err(io_error(&failure, target))
        }
        _ => Ok(()),
    }
}

fn win32_code(result: i32) -> i32 {
    if result.cast_unsigned() & 0xffff_0000 == 0x8007_0000 {
        result & 0xffff
    } else {
        result
    }
}

fn retries_buffered(flags: u32, code: i32) -> bool {
    flags & COPY_FILE_NO_BUFFERING != 0
        && [
            ERROR_INVALID_PARAMETER,
            ERROR_NOT_SUPPORTED,
            ERROR_INVALID_FUNCTION,
        ]
        .contains(&code.cast_unsigned())
}

fn may_be_source(kind: FsErrorKind) -> bool {
    matches!(
        kind,
        FsErrorKind::SharingViolation
            | FsErrorKind::Locked
            | FsErrorKind::NotFound
            | FsErrorKind::AccessDenied
    )
}

fn copy_error(code: i32, source_path: &Path, paths: &Paths<'_>) -> FsError {
    let failure = io::Error::from_raw_os_error(code);
    if may_be_source(FsErrorKind::from_windows_code(code.cast_unsigned()))
        && let Err(source_failure) = File::open(source_path)
    {
        return io_error(&source_failure, paths.source);
    }
    let mut located = io_error(&failure, paths.target);
    if code == ERROR_REQUEST_ABORTED.cast_signed() {
        located.kind = FsErrorKind::Cancelled;
    }
    located
}

unsafe extern "system" fn on_progress(
    message: *const COPYFILE2_MESSAGE,
    context: *const c_void,
) -> COPYFILE2_MESSAGE_ACTION {
    // SAFETY: CopyFile2 returns our live, exclusive CopyProgress pointer for each synchronous callback.
    let progress = unsafe { &mut *context.cast_mut().cast::<CopyProgress<'_>>() };
    if progress.cancel.is_cancelled() || progress.panic.is_some() {
        return COPYFILE2_PROGRESS_CANCEL;
    }
    // SAFETY: CopyFile2 supplies a valid message for the duration of this callback.
    let message = unsafe { &*message };
    match catch_unwind(AssertUnwindSafe(|| progress.on_message(message))) {
        Ok(true) if !progress.cancel.is_cancelled() => COPYFILE2_PROGRESS_CONTINUE,
        Ok(_) => COPYFILE2_PROGRESS_CANCEL,
        Err(payload) => {
            progress.panic = Some(payload);
            COPYFILE2_PROGRESS_CANCEL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_unbuffered_copy_rejected_for_its_flag_retries_buffered() {
        let unbuffered = COPY_FILE_FAIL_IF_EXISTS | COPY_FILE_NO_BUFFERING;
        for code in [87, 50, 1] {
            assert!(retries_buffered(unbuffered, code));
            assert!(!retries_buffered(COPY_FILE_FAIL_IF_EXISTS, code));
        }
        for code in [5, 32, 80, 112, 1235] {
            assert!(!retries_buffered(unbuffered, code));
        }
    }

    #[test]
    fn hresults_unwrap_to_win32_codes() {
        assert_eq!(win32_code(0x8007_0057_u32.cast_signed()), 87);
        assert_eq!(
            win32_code(0x8000_4005_u32.cast_signed()),
            0x8000_4005_u32.cast_signed()
        );
    }
}
