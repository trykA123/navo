mod demangle;
mod lines;
mod names;
mod parse;
mod query;
pub mod sample;
mod symbols;

use std::fmt;
use std::ops::Range;

use object::ReadCache;
use object::read::ReadCacheOps;

pub use lines::{Lines, SourceLine};
pub use query::{Column, Order, Query, Scope, select};

use crate::{IndexError, Watch};

pub const ELF_EXTENSIONS: [&str; 3] = ["elf", "axf", "out"];

const MAGIC: &[u8; 4] = b"\x7FELF";

#[must_use]
pub fn sniff_elf(head: &[u8]) -> bool {
    head.starts_with(MAGIC)
        && matches!(head.get(4), Some(1 | 2))
        && matches!(head.get(5), Some(1 | 2))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Elf32,
    Elf64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub class: Class,
    pub endian: Endian,
    pub machine: u16,
    pub kind: u16,
    pub entry: u64,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub kind: u32,
    pub flags: u64,
    pub address: u64,
    pub size: u64,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub virtual_address: u64,
    pub physical_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind {
    None,
    Object,
    Func,
    Common,
    Tls,
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bind {
    Local,
    Global,
    Weak,
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    Undefined,
    Absolute,
    Common,
    Section(u32),
    Other(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    name: Range<u32>,
    raw: Option<Range<u32>>,
    pub value: u64,
    pub size: u64,
    pub kind: SymbolKind,
    pub bind: Bind,
    pub place: Place,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub load: u64,
    pub ram: u64,
    pub from_segments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElfProblem {
    Stripped,
    NoDwarf,
    CompressedDwarf,
    Malformed(String),
    TooMany {
        what: &'static str,
        kept: u64,
        total: u64,
    },
}

impl fmt::Display for ElfProblem {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stripped => out.write_str("stripped: no symbol table"),
            Self::NoDwarf => out.write_str("no DWARF line info"),
            Self::CompressedDwarf => out.write_str("compressed DWARF is not read"),
            Self::Malformed(what) => write!(out, "malformed: {what}"),
            Self::TooMany { what, kept, total } => {
                write!(out, "{what}: showing {kept} of {total}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elf {
    pub header: Header,
    pub sections: Vec<Section>,
    pub segments: Vec<Segment>,
    pub symbols: Vec<Symbol>,
    pub problems: Vec<ElfProblem>,
    pub totals: Totals,
    names: String,
    dwarf: Vec<(&'static str, usize)>,
}

impl Elf {
    #[must_use]
    pub fn name(&self, symbol: &Symbol) -> &str {
        self.text(&symbol.name)
    }

    #[must_use]
    pub fn raw_name(&self, symbol: &Symbol) -> Option<&str> {
        symbol.raw.as_ref().map(|range| self.text(range))
    }

    fn text(&self, range: &Range<u32>) -> &str {
        self.names
            .get(range.start as usize..range.end as usize)
            .unwrap_or("")
    }

    #[must_use]
    pub fn section_of(&self, symbol: &Symbol) -> Option<&Section> {
        match symbol.place {
            Place::Section(index) => self.sections.get(index as usize),
            _ => None,
        }
    }

    #[must_use]
    pub fn has_dwarf(&self) -> bool {
        self.dwarf.iter().any(|(name, _)| *name == ".debug_line")
    }

    pub fn load_segments(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter().filter(|segment| segment.is_load())
    }

    #[must_use]
    pub fn machine_name(&self) -> String {
        names::machine(self.header.machine)
    }

    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        names::file_kind(self.header.kind)
    }
}

impl Section {
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.flags & u64::from(object::elf::SHF_ALLOC) != 0
    }

    #[must_use]
    pub fn flag_letters(&self) -> String {
        names::section_flags(self.flags)
    }

    #[must_use]
    pub fn kind_name(&self) -> String {
        names::section_kind(self.kind)
    }

    #[must_use]
    pub fn span(&self) -> Range<u64> {
        self.address..self.address.saturating_add(self.size)
    }
}

impl Segment {
    #[must_use]
    pub fn is_load(&self) -> bool {
        self.kind == object::elf::PT_LOAD
    }

    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.flags & object::elf::PF_W != 0
    }

    #[must_use]
    pub fn flag_letters(&self) -> String {
        names::segment_flags(self.flags)
    }

    #[must_use]
    pub fn span(&self) -> Range<u64> {
        self.virtual_address..self.virtual_address.saturating_add(self.memory_size)
    }
}

impl SymbolKind {
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::None => "none".to_owned(),
            Self::Object => "object".to_owned(),
            Self::Func => "func".to_owned(),
            Self::Common => "common".to_owned(),
            Self::Tls => "tls".to_owned(),
            Self::Other(kind) => format!("type {kind}"),
        }
    }
}

impl Bind {
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Local => "local".to_owned(),
            Self::Global => "global".to_owned(),
            Self::Weak => "weak".to_owned(),
            Self::Other(bind) => format!("bind {bind}"),
        }
    }
}

pub struct ElfReader<R: ReadCacheOps> {
    cache: ReadCache<R>,
}

impl<R: ReadCacheOps> ElfReader<R> {
    pub fn new(source: R) -> Self {
        Self {
            cache: ReadCache::new(source),
        }
    }

    pub fn parse(&self, watch: &mut Watch<'_>) -> Result<Elf, IndexError> {
        parse::parse(&self.cache, watch)
    }

    pub fn lines(&self, elf: &Elf, watch: &mut Watch<'_>) -> Result<Lines, IndexError> {
        lines::read(&self.cache, elf, watch)
    }
}
