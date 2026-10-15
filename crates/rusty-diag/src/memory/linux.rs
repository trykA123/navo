use std::io;

use super::{MemoryProbe, ProcessMemory};

pub struct ProcStatusProbe;

impl MemoryProbe for ProcStatusProbe {
    fn sample(&self) -> io::Result<ProcessMemory> {
        todo!()
    }
}

pub fn parse_proc_status(text: &str) -> Option<ProcessMemory> {
    todo!()
}
