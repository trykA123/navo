use std::ptr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::WNet::{
    CONNECT_INTERACTIVE, NETRESOURCEW, RESOURCE_REMEMBERED, RESOURCETYPE_DISK, WNetAddConnection2W,
    WNetCloseEnum, WNetEnumResourceW, WNetGetConnectionW, WNetOpenEnumW,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

use super::VolumeIdentity;
use super::model::{DriveKind, RawDrives, Space};

const NAME_LEN: usize = 261;
const ENUM_BUFFER: usize = 16 * 1024;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

fn kind_of(code: u32) -> DriveKind {
    match code {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Network,
        5 => DriveKind::Optical,
        6 => DriveKind::Ram,
        _ => DriveKind::Unknown,
    }
}

fn letters(mask: u32) -> impl Iterator<Item = char> {
    (0u8..26)
        .filter(move |bit| mask & (1 << bit) != 0)
        .map(|bit| char::from(b'A' + bit))
}

const ERROR_CONNECTION_UNAVAIL: u32 = 1201;
const REMEMBERED_EVERY: Duration = Duration::from_secs(30);

type Remembered = Vec<(char, String)>;

static REMEMBERED: Mutex<Option<(Instant, Remembered)>> = Mutex::new(None);

fn connection(letter: char) -> Option<(String, bool)> {
    let local = wide(&format!("{letter}:"));
    let mut remote = vec![0u16; NAME_LEN];
    let mut len = u32::try_from(remote.len()).unwrap_or(0);
    // SAFETY: `local` is NUL-terminated, `remote` holds `len` units and `len` is a valid pointer.
    let status = unsafe { WNetGetConnectionW(local.as_ptr(), remote.as_mut_ptr(), &raw mut len) };
    match status {
        NO_ERROR => Some((from_wide(&remote), true)),
        ERROR_CONNECTION_UNAVAIL => Some((from_wide(&remote), false)),
        _ => None,
    }
}

fn remembered_cached() -> Remembered {
    let mut cached = REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((read_at, list)) = cached.as_ref()
        && read_at.elapsed() < REMEMBERED_EVERY
    {
        return list.clone();
    }
    let list = remembered();
    *cached = Some((Instant::now(), list.clone()));
    list
}

fn remembered() -> Vec<(char, String)> {
    let mut handle: HANDLE = ptr::null_mut();
    // SAFETY: a null resource asks for the top level; `handle` receives the enumeration handle.
    let opened = unsafe {
        WNetOpenEnumW(
            RESOURCE_REMEMBERED,
            RESOURCETYPE_DISK,
            0,
            ptr::null(),
            &raw mut handle,
        )
    };
    if opened != NO_ERROR {
        return Vec::new();
    }
    let found = read_enum(handle);
    // SAFETY: `handle` came from a successful `WNetOpenEnumW` and is closed once.
    unsafe { WNetCloseEnum(handle) };
    found
}

fn read_enum(handle: HANDLE) -> Vec<(char, String)> {
    let mut found = Vec::new();
    let count_per_item = std::mem::size_of::<NETRESOURCEW>();
    let mut buffer = vec![0u64; ENUM_BUFFER / 8];
    loop {
        let mut count = u32::MAX;
        let mut size = u32::try_from(buffer.len() * 8).unwrap_or(0);
        // SAFETY: `buffer` is 8-byte aligned and `size` bytes long; Windows fills it with
        // NETRESOURCEW records whose strings point inside the same buffer.
        let status = unsafe {
            WNetEnumResourceW(
                handle,
                &raw mut count,
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        if status != NO_ERROR {
            return found;
        }
        let items = usize::try_from(count)
            .unwrap_or(0)
            .min(ENUM_BUFFER / count_per_item);
        let records = buffer.as_ptr().cast::<NETRESOURCEW>();
        for index in 0..items {
            // SAFETY: Windows wrote `count` records at the start of `buffer`.
            let record = unsafe { &*records.add(index) };
            if let Some(entry) = remembered_entry(record) {
                found.push(entry);
            }
        }
    }
}

fn read_pwstr(text: *const u16) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let mut len = 0;
    // SAFETY: Windows returns NUL-terminated strings inside the enumeration buffer.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units before the NUL were just read from the same string.
    let units = unsafe { std::slice::from_raw_parts(text, len) };
    Some(String::from_utf16_lossy(units))
}

fn remembered_entry(record: &NETRESOURCEW) -> Option<(char, String)> {
    let local = read_pwstr(record.lpLocalName)?;
    let remote = read_pwstr(record.lpRemoteName)?;
    let letter = local.chars().next().filter(char::is_ascii_alphabetic)?;
    Some((letter.to_ascii_uppercase(), remote))
}

pub fn raw_drives() -> RawDrives {
    // SAFETY: takes no arguments and only returns a bit mask.
    let mask = unsafe { GetLogicalDrives() };
    let mut raw = RawDrives {
        mask,
        ..RawDrives::default()
    };
    for letter in letters(mask) {
        let root = wide(&format!("{letter}:\\"));
        // SAFETY: `root` is a NUL-terminated drive root.
        let kind = kind_of(unsafe { GetDriveTypeW(root.as_ptr()) });
        raw.kinds.push((letter, kind));
        if kind == DriveKind::Network
            && let Some((remote, available)) = connection(letter)
        {
            raw.mapped.push((letter, remote));
            if !available {
                raw.unavailable.push(letter);
            }
        }
    }
    raw.remembered = remembered_cached();
    raw
}

pub fn details(root: &str) -> (Option<String>, Option<Space>) {
    let root = wide(root);
    let mut name = vec![0u16; NAME_LEN];
    // SAFETY: `root` is NUL-terminated and `name` holds NAME_LEN units; other outputs are unused.
    let labelled = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            name.as_mut_ptr(),
            u32::try_from(name.len()).unwrap_or(0),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        )
    };
    let label = (labelled != 0)
        .then(|| from_wide(&name))
        .filter(|label| !label.is_empty());
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: `root` is NUL-terminated and both outputs point to live u64 values.
    let sized = unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            &raw mut free,
            &raw mut total,
            ptr::null_mut(),
        )
    };
    let space = (sized != 0).then_some(Space { total, free });
    (label, space)
}

pub fn identity(letter: char) -> Option<VolumeIdentity> {
    let root = wide(&format!("{letter}:\\"));
    // SAFETY: `root` is a NUL-terminated drive root.
    if kind_of(unsafe { GetDriveTypeW(root.as_ptr()) }) == DriveKind::Network {
        return connection(letter).map(|(remote, _)| VolumeIdentity::Remote(remote));
    }
    let mut serial = 0u32;
    // SAFETY: `root` is NUL-terminated and `serial` points to a live u32; other outputs are unused.
    let read = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            &raw mut serial,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        )
    };
    (read != 0).then_some(VolumeIdentity::Serial(serial))
}

pub fn connect(letter: char, remote: &str) -> Result<(), String> {
    let mut local = wide(&format!("{letter}:"));
    let mut remote_wide = wide(remote);
    let resource = NETRESOURCEW {
        dwType: RESOURCETYPE_DISK,
        lpLocalName: local.as_mut_ptr(),
        lpRemoteName: remote_wide.as_mut_ptr(),
        ..NETRESOURCEW::default()
    };
    // SAFETY: `resource` points at NUL-terminated buffers that outlive the call.
    let status = unsafe {
        WNetAddConnection2W(
            &raw const resource,
            ptr::null(),
            ptr::null(),
            CONNECT_INTERACTIVE,
        )
    };
    if status == NO_ERROR {
        Ok(())
    } else {
        Err(format!(
            "Could not reconnect {letter}: (Windows error {status})."
        ))
    }
}
