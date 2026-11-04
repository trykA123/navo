#[cfg(windows)]
mod windows;

use std::path::Path;

use serde::Serialize;

const GIB: u64 = 1 << 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiskKind {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefenderState {
    On,
    Off,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FreeSpace {
    Under10Gib,
    From10To50Gib,
    From50To200Gib,
    Over200Gib,
    Unknown,
}

impl FreeSpace {
    pub fn from_bytes(bytes: u64) -> Self {
        match bytes / GIB {
            0..=9 => Self::Under10Gib,
            10..=49 => Self::From10To50Gib,
            50..=199 => Self::From50To200Gib,
            _ => Self::Over200Gib,
        }
    }
}

/// Hardware and OS facts with no machine name, user name or serial.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct MachineSummary {
    pub cpu_cores: u32,
    pub ram_gib: Option<u32>,
    pub disk: DiskKind,
    pub free_space: FreeSpace,
    pub windows_build: Option<u32>,
    pub defender: DefenderState,
}

pub(crate) fn ram_gib(bytes: u64) -> u32 {
    u32::try_from(bytes.div_ceil(GIB)).unwrap_or(u32::MAX)
}

/// Drive letter of a path such as `C:\x` or `\\?\C:\x`; none for other forms.
#[cfg(any(windows, test))]
pub(crate) fn drive_letter(path: &str) -> Option<char> {
    let rest = path.strip_prefix(r"\\?\").unwrap_or(path).as_bytes();
    match rest {
        [letter, b':', ..] if letter.is_ascii_alphabetic() => {
            Some(letter.to_ascii_uppercase() as char)
        }
        _ => None,
    }
}

fn cpu_cores() -> u32 {
    std::thread::available_parallelism()
        .map_or(1, |count| u32::try_from(count.get()).unwrap_or(u32::MAX))
}

/// Reads the machine facts for the drive that holds `test_root`. Never needs admin.
pub fn summarize(test_root: &Path) -> MachineSummary {
    let probe = probe(test_root);
    MachineSummary {
        cpu_cores: cpu_cores(),
        ram_gib: probe.ram_bytes.map(ram_gib),
        disk: probe.disk,
        free_space: probe
            .free_bytes
            .map_or(FreeSpace::Unknown, FreeSpace::from_bytes),
        windows_build: probe.windows_build,
        defender: probe.defender,
    }
}

pub(crate) struct Probe {
    pub(crate) ram_bytes: Option<u64>,
    pub(crate) disk: DiskKind,
    pub(crate) free_bytes: Option<u64>,
    pub(crate) windows_build: Option<u32>,
    pub(crate) defender: DefenderState,
}

#[cfg(windows)]
fn probe(test_root: &Path) -> Probe {
    windows::probe(test_root)
}

#[cfg(not(windows))]
fn probe(_test_root: &Path) -> Probe {
    Probe {
        ram_bytes: None,
        disk: DiskKind::Unknown,
        free_bytes: None,
        windows_build: None,
        defender: DefenderState::Unknown,
    }
}

#[cfg(test)]
mod tests;
