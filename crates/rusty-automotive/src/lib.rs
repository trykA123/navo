#![forbid(unsafe_code)]

mod elf;
mod image;
mod layout;
mod lines;
mod record;
mod scan;
mod source;

use std::sync::atomic::AtomicBool;

#[cfg(feature = "sample")]
pub use elf::sample as elf_sample;
pub use elf::{
    Bind, Class, Column, Elf, ElfProblem, ElfReader, Endian, Header, Lines, Order, Place, Query,
    Scope, Section, Segment, SourceLine, Symbol, SymbolKind, Totals, select, sniff_elf,
};
pub use image::{Counts, Image, Item, Problem, Start};
pub use layout::{Layout, ROW_BYTES, Row};
pub use scan::{MAX_PIECES, index};
pub use source::Source;

pub type ReadAt<'a> = dyn FnMut(u64, usize) -> Result<Vec<u8>, String> + 'a;

pub const EXTENSIONS: [&str; 8] = ["hex", "ihex", "s19", "s28", "s37", "srec", "mot", "sx"];

pub struct Watch<'a> {
    pub cancel: &'a AtomicBool,
    pub progress: &'a mut dyn FnMut(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    #[error("reading the file: {0}")]
    Read(String),
    #[error("indexing was cancelled")]
    Cancelled,
    #[error("not a readable ELF file: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    IntelHex,
    SRecord,
}

const INTEL_RECORDS: [&str; 6] = [
    "Data",
    "End of file",
    "Extended segment address",
    "Start segment address",
    "Extended linear address",
    "Start linear address",
];

const S_RECORDS: [&str; 10] = [
    "Header",
    "Data, 16-bit address",
    "Data, 24-bit address",
    "Data, 32-bit address",
    "Reserved",
    "Record count, 16-bit",
    "Record count, 24-bit",
    "Start address, 32-bit",
    "Start address, 24-bit",
    "Start address, 16-bit",
];

impl Format {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::IntelHex => "Intel HEX",
            Self::SRecord => "Motorola S-record",
        }
    }

    #[must_use]
    pub fn record_kinds(self) -> u8 {
        match self {
            Self::IntelHex => 6,
            Self::SRecord => 10,
        }
    }

    #[must_use]
    pub fn record_label(self, kind: u8) -> String {
        let at = usize::from(kind);
        match self {
            Self::IntelHex => format!("{kind:02} {}", INTEL_RECORDS.get(at).unwrap_or(&"")),
            Self::SRecord => format!("S{kind} {}", S_RECORDS.get(at).unwrap_or(&"")),
        }
    }
}

#[must_use]
pub fn sniff(head: &[u8]) -> Option<Format> {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let first = head
        .split(|byte| lines::is_line_end(*byte))
        .map(<[u8]>::trim_ascii)
        .find(|line| !line.is_empty())?;
    let mut bytes = Vec::new();
    [Format::IntelHex, Format::SRecord]
        .into_iter()
        .find(|format| record::decode(*format, first, &mut bytes).is_ok())
}
