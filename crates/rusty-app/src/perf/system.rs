use rusty_diag::perf::SystemFacts;

pub(super) fn facts() -> SystemFacts {
    let base = crate::diag::report_input().system;
    SystemFacts {
        os: os_name().unwrap_or(base.os),
        cpu_model: cpu_model().unwrap_or_default(),
        logical_cores: base.cpu_count,
        ram_bytes: base.ram_bytes,
        gpu: None,
        monitors: Vec::new(),
    }
}

#[cfg(any(not(windows), test))]
pub(super) fn pretty_name(os_release: &str) -> Option<String> {
    let line = os_release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))?;
    Some(line.trim().trim_matches('"').to_owned()).filter(|name| !name.is_empty())
}

#[cfg(any(not(windows), test))]
pub(super) fn model_name(cpuinfo: &str) -> Option<String> {
    let line = cpuinfo
        .lines()
        .find(|line| line.starts_with("model name"))?;
    Some(line.split_once(':')?.1.trim().to_owned())
}

#[cfg(not(windows))]
fn os_name() -> Option<String> {
    let name = pretty_name(&std::fs::read_to_string("/etc/os-release").ok()?)?;
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    Some(format!("{name} (kernel {})", kernel.trim()))
}

#[cfg(not(windows))]
fn cpu_model() -> Option<String> {
    model_name(&std::fs::read_to_string("/proc/cpuinfo").ok()?)
}

#[cfg(windows)]
fn os_name() -> Option<String> {
    const KEY: &str = r"software\microsoft\windows nt\currentversion";
    let product = registry::text(KEY, "ProductName")?;
    let release = registry::text(KEY, "DisplayVersion").unwrap_or_default();
    let build = registry::text(KEY, "CurrentBuildNumber").unwrap_or_default();
    let update = registry::number(KEY, "UBR").unwrap_or_default();
    Some(format!("{product} {release} (build {build}.{update})"))
}

#[cfg(windows)]
fn cpu_model() -> Option<String> {
    registry::text(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
    .map(|name| name.trim().to_owned())
}

#[cfg(windows)]
mod registry {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
    };

    const UNITS: usize = 256;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn text(key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        let mut buffer = [0_u16; UNITS];
        let mut size = (UNITS * 2) as u32;
        // SAFETY: the key and value names are NUL-terminated, and `size` is the byte length of the writable `buffer`.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let units = (size as usize / 2).min(UNITS);
        let text = String::from_utf16_lossy(&buffer[..units]);
        Some(text.trim_end_matches('\0').to_owned())
    }

    pub(super) fn number(key: &str, name: &str) -> Option<u32> {
        let (key, name) = (wide(key), wide(name));
        let mut value = 0_u32;
        let mut size = size_of::<u32>() as u32;
        // SAFETY: the key and value names are NUL-terminated and `value` holds the four bytes a DWORD needs.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                null_mut(),
                (&raw mut value).cast(),
                &raw mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(value)
    }
}
