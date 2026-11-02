use std::io;

use super::{MemoryProbe, ProcessMemory};

pub struct ProcStatusProbe;

impl MemoryProbe for ProcStatusProbe {
    fn sample(&self) -> io::Result<ProcessMemory> {
        let text = std::fs::read_to_string("/proc/self/status")?;
        parse_proc_status(&text).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "/proc/self/status has missing or malformed VmRSS or RssAnon",
            )
        })
    }
}

pub fn parse_proc_status(text: &str) -> Option<ProcessMemory> {
    let mut resident_bytes = None;
    let mut private_bytes = None;
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim() {
            "VmRSS" => resident_bytes = Some(parse_kilobytes(value)?),
            "RssAnon" => private_bytes = Some(parse_kilobytes(value)?),
            _ => {}
        }
    }
    Some(ProcessMemory {
        resident_bytes: resident_bytes?,
        private_bytes: private_bytes?,
    })
}

fn parse_kilobytes(text: &str) -> Option<u64> {
    let mut fields = text.split_whitespace();
    let value = fields.next()?.parse::<u64>().ok()?;
    if fields.next()? != "kB" || fields.next().is_some() {
        return None;
    }
    value.checked_mul(1024)
}
