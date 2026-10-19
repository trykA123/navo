#![allow(unsafe_code, reason = "std has no safe wrapper for CopyFile2")]

use std::any::Any;
use std::ffi::c_void;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use rusty_core::{FsError, FsErrorKind, Location};
use windows_sys::Win32::Foundation::ERROR_REQUEST_ABORTED;
use windows_sys::Win32::Storage::FileSystem::{
    COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, COPYFILE2_CALLBACK_CHUNK_FINISHED,
    COPYFILE2_CALLBACK_STREAM_FINISHED, COPYFILE2_EXTENDED_PARAMETERS, COPYFILE2_MESSAGE,
    COPYFILE2_MESSAGE_ACTION, COPYFILE2_PROGRESS_CANCEL, COPYFILE2_PROGRESS_CONTINUE, CopyFile2,
};

use super::{AutoTuning, error, io_error, os_path};
use crate::CancelToken;

pub(super) struct CopyProgress<'a> {
    report: &'a mut dyn FnMut(u64),
    cancel: &'a CancelToken,
    total: u64,
    panic: Option<Box<dyn Any + Send>>,
}

impl<'a> CopyProgress<'a> {
    pub(super) fn new(report: &'a mut dyn FnMut(u64), cancel: &'a CancelToken) -> Self {
        Self {
            report,
            cancel,
            total: 0,
            panic: None,
        }
    }

    fn report_message(&mut self, message: &COPYFILE2_MESSAGE) {
        let (stream, bytes) = match message.Type {
            COPYFILE2_CALLBACK_CHUNK_FINISHED => {
                // SAFETY: Type selects the initialized ChunkFinished union member.
                let info = unsafe { message.Info.ChunkFinished };
                (info.dwStreamNumber, info.uliStreamBytesTransferred)
            }
            COPYFILE2_CALLBACK_STREAM_FINISHED => {
                // SAFETY: Type selects the initialized StreamFinished union member.
                let info = unsafe { message.Info.StreamFinished };
                (info.dwStreamNumber, info.uliStreamBytesTransferred)
            }
            _ => return,
        };
        if stream == 1 {
            self.total = self.total.max(bytes);
            (self.report)(self.total);
        }
    }
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
    let size = fs::metadata(&source_path)
        .map_err(|e| io_error(&e, source))?
        .len();
    let source_wide: Vec<u16> = source_path.as_os_str().encode_wide().chain([0]).collect();
    let target_wide: Vec<u16> = os_path(target)?
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    let mut flags = COPY_FILE_FAIL_IF_EXISTS;
    if size >= tuning.unbuffered_from {
        flags |= COPY_FILE_NO_BUFFERING;
    }
    let parameters = COPYFILE2_EXTENDED_PARAMETERS {
        dwSize: size_of::<COPYFILE2_EXTENDED_PARAMETERS>() as u32,
        dwCopyFlags: flags,
        pfCancel: std::ptr::null_mut(),
        pProgressRoutine: Some(on_progress),
        pvCallbackContext: (&raw mut progress).cast(),
    };
    // SAFETY: paths are NUL-terminated; parameters and exclusive callback state outlive this synchronous call.
    let result = unsafe { CopyFile2(source_wide.as_ptr(), target_wide.as_ptr(), &parameters) };
    if let Some(payload) = progress.panic.take() {
        resume_unwind(payload);
    }
    if result < 0 {
        return Err(copy_error(result, target));
    }
    if progress.cancel.is_cancelled() {
        return Err(error(FsErrorKind::Cancelled, target));
    }
    Ok(progress.total)
}

fn copy_error(result: i32, target: &Location) -> FsError {
    let code = if result.cast_unsigned() & 0xffff_0000 == 0x8007_0000 {
        result & 0xffff
    } else {
        result
    };
    let mut failure = io_error(&io::Error::from_raw_os_error(code), target);
    if code == ERROR_REQUEST_ABORTED.cast_signed() {
        failure.kind = FsErrorKind::Cancelled;
    }
    failure
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
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| progress.report_message(message))) {
        progress.panic = Some(payload);
        return COPYFILE2_PROGRESS_CANCEL;
    }
    if progress.cancel.is_cancelled() {
        COPYFILE2_PROGRESS_CANCEL
    } else {
        COPYFILE2_PROGRESS_CONTINUE
    }
}
