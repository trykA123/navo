#![allow(unsafe_code, reason = "NTFS volume control calls require Win32 FFI")]

use std::{io, ptr};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_HANDLE_EOF, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL,
};

use super::{
    Enumeration, JournalPosition, NtfsError, UsnBatch, VolumeIo, enum_request, enumerate_volume,
    journal_request, read_volume_journal, volume_path,
};

pub struct Volume {
    handle: VolumeHandle,
    drive: char,
}

struct VolumeHandle(HANDLE);

impl Volume {
    pub fn open(drive: char) -> Result<Self, NtfsError> {
        let path = volume_path(drive)?;
        // SAFETY: path is NUL-terminated and lives through this synchronous call; optional pointers are NULL.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(NtfsError::Io {
                operation: "open volume",
                source: io::Error::last_os_error(),
            });
        }
        Ok(Self {
            handle: VolumeHandle(handle),
            drive,
        })
    }

    pub fn enumerate(&mut self) -> Result<Enumeration, NtfsError> {
        let drive = self.drive;
        enumerate_volume(self, drive, usize::MAX)
    }

    pub fn read_journal(&mut self, saved: JournalPosition) -> Result<UsnBatch, NtfsError> {
        read_volume_journal(self, saved)
    }

    fn control(&self, code: u32, input: &[u8], capacity: usize) -> Result<Vec<u8>, NtfsError> {
        let input_size = u32::try_from(input.len())
            .map_err(|_| NtfsError::InvalidResponse("input buffer too large"))?;
        let output_size = u32::try_from(capacity)
            .map_err(|_| NtfsError::InvalidResponse("output buffer too large"))?;
        let mut output = vec![0u8; capacity];
        let mut returned = 0;
        let input_pointer = if input.is_empty() {
            ptr::null()
        } else {
            input.as_ptr().cast()
        };
        // SAFETY: the guard owns a live synchronous volume handle; both byte slices and returned outlive the call and match the supplied lengths.
        let ok = unsafe {
            DeviceIoControl(
                self.handle.0,
                code,
                input_pointer,
                input_size,
                output.as_mut_ptr().cast(),
                output_size,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(NtfsError::Io {
                operation: "volume control",
                source: io::Error::last_os_error(),
            });
        }
        if returned as usize > output.len() {
            return Err(NtfsError::InvalidResponse("returned length exceeds buffer"));
        }
        output.truncate(returned as usize);
        Ok(output)
    }
}

impl VolumeIo for Volume {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        self.control(FSCTL_QUERY_USN_JOURNAL, &[], 56)
    }

    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        match self.control(FSCTL_ENUM_USN_DATA, &enum_request(cursor), 64 * 1024) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(NtfsError::Io { source, .. })
                if source.raw_os_error() == Some(ERROR_HANDLE_EOF as i32) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn read_journal(&mut self, position: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        self.control(
            FSCTL_READ_USN_JOURNAL,
            &journal_request(position),
            64 * 1024,
        )
    }
}

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: this guard owns the valid handle returned by CreateFileW and closes it exactly once.
        unsafe { CloseHandle(self.0) };
    }
}
