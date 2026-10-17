use std::io;
use std::mem::size_of;

use rusty_diag::{MemoryProbe, ProcessMemory};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

pub struct WindowsProbe;

impl MemoryProbe for WindowsProbe {
    fn sample(&self) -> io::Result<ProcessMemory> {
        let counters = counters()?;
        Ok(ProcessMemory {
            resident_bytes: counters.WorkingSetSize as u64,
            private_bytes: counters.PrivateUsage as u64,
        })
    }
}

pub fn peak_working_set() -> Option<u64> {
    counters()
        .ok()
        .map(|counters| counters.PeakWorkingSetSize as u64)
}

fn counters() -> io::Result<PROCESS_MEMORY_COUNTERS_EX> {
    let cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb,
        ..PROCESS_MEMORY_COUNTERS_EX::default()
    };
    let pointer = (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>();
    // SAFETY: the pseudo-handle of the current process needs no closing, and `pointer` is a live, writable PROCESS_MEMORY_COUNTERS_EX of `cb` bytes.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), pointer, cb) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(counters)
}

pub fn total_ram() -> u64 {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: `status` is a live, writable MEMORYSTATUSEX whose dwLength is set, as the call requires.
    let ok = unsafe { GlobalMemoryStatusEx(&raw mut status) };
    if ok == 0 { 0 } else { status.ullTotalPhys }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_reads_this_process() {
        let memory = WindowsProbe.sample().unwrap();
        assert!(memory.resident_bytes > 0 && memory.private_bytes > 0);
    }

    #[test]
    fn the_peak_is_at_least_the_working_set() {
        let memory = WindowsProbe.sample().unwrap();
        assert!(peak_working_set().unwrap() >= memory.resident_bytes);
    }

    #[test]
    fn total_ram_is_known() {
        assert!(total_ram() > 0);
    }
}
