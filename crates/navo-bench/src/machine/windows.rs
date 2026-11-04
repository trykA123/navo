#![allow(unsafe_code, reason = "read-only Win32 queries on fixed local buffers")]

use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
    STORAGE_PROPERTY_QUERY, StorageDeviceSeekPenaltyProperty,
};
use windows_sys::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use super::{DefenderState, DiskKind, Probe, drive_letter};

const NT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const DEFENDER_RTP: &str = r"SOFTWARE\Microsoft\Windows Defender\Real-Time Protection";

pub(super) fn probe(test_root: &Path) -> Probe {
    let letter = drive_letter(&test_root.to_string_lossy());
    Probe {
        ram_bytes: total_ram(),
        disk: letter.map_or(DiskKind::Unknown, disk_kind),
        free_bytes: letter.and_then(free_bytes),
        windows_build: registry_text(NT_VERSION, "CurrentBuildNumber")
            .and_then(|text| text.parse().ok()),
        defender: defender_state(),
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn total_ram() -> Option<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: `status` is a live struct with its length field set.
    let ok = unsafe { GlobalMemoryStatusEx(&raw mut status) };
    (ok != 0).then_some(status.ullTotalPhys)
}

fn free_bytes(letter: char) -> Option<u64> {
    let root = wide(&format!(r"{letter}:\"));
    let mut available = 0_u64;
    // SAFETY: `root` is NUL-terminated; unused outputs may be null.
    let ok =
        unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &raw mut available, null_mut(), null_mut()) };
    (ok != 0).then_some(available)
}

fn disk_kind(letter: char) -> DiskKind {
    let name = wide(&format!(r"\\.\{letter}:"));
    // SAFETY: zero access rights query a volume without admin; `name` is NUL-terminated.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return DiskKind::Unknown;
    }
    let kind = seek_penalty(handle);
    // SAFETY: `handle` is open and owned here.
    unsafe { CloseHandle(handle) };
    kind
}

fn seek_penalty(handle: windows_sys::Win32::Foundation::HANDLE) -> DiskKind {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        ..STORAGE_PROPERTY_QUERY::default()
    };
    let mut out = DEVICE_SEEK_PENALTY_DESCRIPTOR {
        Version: 0,
        Size: 0,
        IncursSeekPenalty: false,
    };
    let mut returned = 0_u32;
    let (Ok(in_len), Ok(out_len)) = (
        u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>()),
        u32::try_from(size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>()),
    ) else {
        return DiskKind::Unknown;
    };
    // SAFETY: both buffers are live, sized as passed, and the call is synchronous.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const query).cast(),
            in_len,
            (&raw mut out).cast(),
            out_len,
            &raw mut returned,
            null_mut(),
        )
    };
    match (ok != 0 && returned >= out_len, out.IncursSeekPenalty) {
        (false, _) => DiskKind::Unknown,
        (true, true) => DiskKind::Hdd,
        (true, false) => DiskKind::Ssd,
    }
}

fn registry_text(key: &str, value: &str) -> Option<String> {
    let (key, value) = (wide(key), wide(value));
    let mut buffer = [0_u16; 32];
    let mut size = u32::try_from(size_of_val(&buffer)).ok()?;
    // SAFETY: names are NUL-terminated; the buffer size in bytes is passed in `size`.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    let units = usize::try_from(size).ok()? / 2;
    let text = buffer.get(..units.saturating_sub(1))?;
    (status == 0).then(|| String::from_utf16_lossy(text))
}

fn registry_dword(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (wide(key), wide(value));
    let mut data = 0_u32;
    let mut size = 4_u32;
    // SAFETY: names are NUL-terminated; `data` holds the four bytes of a DWORD.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast(),
            &raw mut size,
        )
    };
    (status == 0).then_some(data)
}

/// Unreadable or absent values stay unknown; tamper protection may hide the key.
fn defender_state() -> DefenderState {
    match registry_dword(DEFENDER_RTP, "DisableRealtimeMonitoring") {
        Some(0) => DefenderState::On,
        Some(_) => DefenderState::Off,
        None => DefenderState::Unknown,
    }
}
