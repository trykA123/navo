#![allow(unsafe_code, reason = "NTFS volume control calls require Win32 FFI")]

use std::{io, ptr};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_HANDLE_EOF, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_DESCRIPTOR, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdType, GetFileSizeEx, GetFinalPathNameByHandleW,
    GetVolumeInformationByHandleW, OPEN_EXISTING, OpenFileById, VOLUME_NAME_NONE,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    FSCTL_CREATE_USN_JOURNAL, FSCTL_ENUM_USN_DATA, FSCTL_GET_NTFS_FILE_RECORD,
    FSCTL_GET_NTFS_VOLUME_DATA, FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL,
};

use super::{
    Continuity, Enumeration, JournalPosition, NtfsError, SizeHint, UsnBatch, VolumeIo,
    enum_request, enumerate_volume, enumerate_volume_with_continuity, journal_request,
    read_volume_journal, size_error, volume_path,
};

pub struct Volume {
    handle: VolumeHandle,
    drive: char,
    record_buffer: Vec<u8>,
}

struct VolumeHandle(HANDLE);

impl Volume {
    pub fn open(drive: char) -> Result<Self, NtfsError> {
        let mut volume = Self {
            handle: VolumeHandle::open_volume(drive, GENERIC_READ, "open volume")?,
            drive,
            record_buffer: Vec::new(),
        };
        let data = volume.control(FSCTL_GET_NTFS_VOLUME_DATA, &[], 96)?;
        let size = u32::from_le_bytes(super::field(&data, 48)?) as usize;
        if !(512..=65536).contains(&size) {
            return Err(NtfsError::InvalidResponse("invalid MFT record size"));
        }
        volume.record_buffer.resize(size + 16, 0);
        Ok(volume)
    }

    pub fn enumerate(
        &mut self,
        cancel: &crate::pipe::CancelToken,
    ) -> Result<Enumeration, NtfsError> {
        let drive = self.drive;
        enumerate_volume(self, drive, usize::MAX, cancel)
    }

    /// Builds the whole volume with a size hint; `continuity` decides whether a journal wrap fails it.
    pub fn enumerate_sized(
        &mut self,
        hint: SizeHint,
        continuity: Continuity,
        cancel: &crate::pipe::CancelToken,
    ) -> Result<Enumeration, NtfsError> {
        let drive = self.drive;
        enumerate_volume_with_continuity(self, drive, hint, continuity, cancel)
    }

    pub fn read_journal(&mut self, saved: JournalPosition) -> Result<UsnBatch, NtfsError> {
        read_volume_journal(self, saved)
    }

    /// Lists the batch's parent folders the index lacks whose MFT parent chain reaches `$Extend`,
    /// for [`super::NtfsIndex::note_metadata_folders`]. A read error ends the search (logged).
    pub fn metadata_parents(&mut self, batch: &UsnBatch, index: &super::NtfsIndex) -> Vec<u128> {
        super::volume_io::extend_chain::metadata_parents(self, batch, index)
    }

    /// MFT record slots right now; read again at each build since the MFT only grows.
    pub fn mft_records(&self) -> Result<u64, NtfsError> {
        super::mft_records(&self.control(FSCTL_GET_NTFS_VOLUME_DATA, &[], 96)?)
    }

    /// Queries the USN journal; fails with the OS error, as 1179 (`ERROR_JOURNAL_NOT_ACTIVE`).
    pub fn query_journal_status(&self) -> Result<(), NtfsError> {
        self.control(FSCTL_QUERY_USN_JOURNAL, &[], 56).map(drop)
    }

    /// Creates the volume's USN journal with a size limit and growth step in bytes, through a
    /// separate read-write volume handle as `fsutil` uses. Fails with the OS error (5 unelevated).
    pub fn create_journal(
        &self,
        maximum_size: u64,
        allocation_delta: u64,
    ) -> Result<(), NtfsError> {
        let handle =
            VolumeHandle::open_volume(self.drive, GENERIC_READ | GENERIC_WRITE, "open volume rw")?;
        let mut input = [0u8; 16];
        input[..8].copy_from_slice(&maximum_size.to_le_bytes());
        input[8..].copy_from_slice(&allocation_delta.to_le_bytes());
        handle
            .control(FSCTL_CREATE_USN_JOURNAL, &input, &mut [])
            .map(drop)
    }

    pub fn serial(&self) -> Result<u64, NtfsError> {
        let mut serial = 0;
        // SAFETY: the volume handle is live; serial is writable; optional buffers are NULL.
        let ok = unsafe {
            GetVolumeInformationByHandleW(
                self.handle.0,
                ptr::null_mut(),
                0,
                &mut serial,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            return Err(NtfsError::Io {
                operation: "read opened volume serial",
                source: io::Error::last_os_error(),
            });
        }
        Ok(u64::from(serial))
    }

    /// Reads up to capacity bytes of MFT records from the cursor, or returns None at EOF.
    fn enumerate_from(&self, cursor: u64, capacity: usize) -> Result<Option<Vec<u8>>, NtfsError> {
        match self.control(FSCTL_ENUM_USN_DATA, &enum_request(cursor), capacity) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(NtfsError::Io { source, .. })
                if source.raw_os_error() == Some(ERROR_HANDLE_EOF as i32) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn control(&self, code: u32, input: &[u8], capacity: usize) -> Result<Vec<u8>, NtfsError> {
        let mut output = vec![0; capacity];
        let returned = self.handle.control(code, input, &mut output)?;
        output.truncate(returned);
        Ok(output)
    }
}

impl VolumeHandle {
    /// Opens the volume device of a drive letter with the given access, sharing everything.
    fn open_volume(drive: char, access: u32, operation: &'static str) -> Result<Self, NtfsError> {
        let path = volume_path(drive)?;
        // SAFETY: path is NUL-terminated and lives through this synchronous call; optional pointers are NULL.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(NtfsError::Io {
                operation,
                source: io::Error::last_os_error(),
            });
        }
        Ok(Self(handle))
    }

    fn control(&self, code: u32, input: &[u8], output: &mut [u8]) -> Result<usize, NtfsError> {
        let input_size = u32::try_from(input.len())
            .map_err(|_| NtfsError::InvalidResponse("input buffer too large"))?;
        let output_size = u32::try_from(output.len())
            .map_err(|_| NtfsError::InvalidResponse("output buffer too large"))?;
        let mut returned = 0;
        let input_pointer = if input.is_empty() {
            ptr::null()
        } else {
            input.as_ptr().cast()
        };
        // SAFETY: the guard owns a live synchronous volume handle; both byte slices and returned outlive the call and match the supplied lengths.
        let ok = unsafe {
            DeviceIoControl(
                self.0,
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
        Ok(returned as usize)
    }
}

impl VolumeIo for Volume {
    fn file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
        let reference = u64::try_from(file_id)
            .map_err(|_| NtfsError::InvalidResponse("wide NTFS reference"))?;
        let ordinal = reference & 0x0000_ffff_ffff_ffff;
        let returned = self
            .handle
            .control(
                FSCTL_GET_NTFS_FILE_RECORD,
                &ordinal.to_le_bytes(),
                &mut self.record_buffer,
            )
            .map_err(size_error)?;
        match super::record_size::record_size(&mut self.record_buffer[..returned], reference)? {
            Some(size) => Ok(size),
            None => self.current_file_size(file_id),
        }
    }

    fn current_file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
        let reference = u64::try_from(file_id)
            .map_err(|_| NtfsError::InvalidResponse("wide NTFS reference"))?;
        let handle = self
            .handle
            .open_by_id(reference, "open file for size")
            .map_err(size_error)?;
        let mut size = 0;
        // SAFETY: handle is live and size is a writable LARGE_INTEGER.
        if unsafe { GetFileSizeEx(handle.0, &mut size) } == 0 {
            return Err(NtfsError::Io {
                operation: "read file size",
                source: io::Error::last_os_error(),
            });
        }
        u64::try_from(size).map_err(|_| NtfsError::InvalidResponse("negative file size"))
    }

    /// Opens the file by its full reference and reads its path without the drive letter.
    fn path_by_id(&mut self, file_id: u128) -> Result<Option<Vec<u16>>, NtfsError> {
        let reference = u64::try_from(file_id)
            .map_err(|_| NtfsError::InvalidResponse("wide NTFS reference"))?;
        let handle = self.handle.open_by_id(reference, "open folder for path")?;
        let mut path = vec![0u16; 1024];
        // SAFETY: handle is live and path is a writable buffer of the length passed.
        let written = unsafe {
            GetFinalPathNameByHandleW(handle.0, path.as_mut_ptr(), 1024, VOLUME_NAME_NONE)
        };
        if written == 0 {
            return Err(NtfsError::Io {
                operation: "read folder path",
                source: io::Error::last_os_error(),
            });
        }
        // A result of the buffer length or more is the size it needs: an empty path, no match.
        let length = usize::try_from(written).unwrap_or(usize::MAX);
        path.truncate(if length < path.len() { length } else { 0 });
        Ok(Some(path))
    }

    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        self.control(FSCTL_QUERY_USN_JOURNAL, &[], 56)
    }

    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.enumerate_from(cursor, 64 * 1024)
    }

    /// Reads the MFT at the cursor with space for a maximum-length USN v3 name.
    fn enumerate_one(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.enumerate_from(cursor, 1024)
    }

    fn read_journal(&mut self, position: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        const JOURNAL_BUFFER: usize = 256 * 1024;
        self.control(
            FSCTL_READ_USN_JOURNAL,
            &journal_request(position),
            JOURNAL_BUFFER,
        )
    }
}

impl VolumeHandle {
    /// Opens a file or folder on this volume by its full 64-bit reference (sequence included), for
    /// attribute reads only. Fails with the OS error, as a stale reference or a denied open.
    fn open_by_id(&self, reference: u64, operation: &'static str) -> Result<Self, NtfsError> {
        let mut descriptor = FILE_ID_DESCRIPTOR {
            dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: FileIdType,
            ..Default::default()
        };
        descriptor.Anonymous.FileId = reference as i64;
        // SAFETY: descriptor and the synchronous volume handle remain live through the call.
        let handle = unsafe {
            OpenFileById(
                self.0,
                &descriptor,
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                FILE_FLAG_BACKUP_SEMANTICS
                    | FILE_FLAG_OPEN_REPARSE_POINT
                    | FILE_FLAG_OPEN_NO_RECALL,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(NtfsError::Io {
                operation,
                source: io::Error::last_os_error(),
            });
        }
        Ok(Self(handle))
    }
}

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: this guard owns the valid handle returned by CreateFileW and closes it exactly once.
        unsafe { CloseHandle(self.0) };
    }
}
