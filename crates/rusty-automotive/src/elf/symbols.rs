use std::ops::Range;
use std::sync::atomic::Ordering;

use object::elf;
use object::read::ReadCacheOps;
use object::read::elf::{SectionHeader, Sym};
use object::{Endianness, Pod};

use super::demangle::demangle;
use super::parse::{Data, bytes_of, name_at};
use super::{Bind, ElfProblem, Place, Section, Symbol, SymbolKind};
use crate::{IndexError, Watch};

const MAX_SYMBOLS: usize = 1_000_000;
const MAX_NAMES: usize = 64 * 1024 * 1024;
const CHECK_EVERY: usize = 8_192;

pub(super) struct SymbolTable<'a, S, R: ReadCacheOps> {
    pub endian: Endianness,
    pub headers: &'a [S],
    pub sections: &'a [Section],
    pub data: Data<'a, R>,
}

#[derive(Default)]
struct Names {
    text: String,
    is_full: bool,
    bad: u64,
}

impl Names {
    fn report(&self, problems: &mut Vec<ElfProblem>) {
        if self.bad > 0 {
            problems.push(ElfProblem::Malformed(format!(
                "{} symbol names point outside the string table",
                self.bad
            )));
        }
        if self.is_full {
            problems.push(ElfProblem::Malformed(format!(
                "symbol names exceed {} MB; the rest are blank",
                MAX_NAMES / 1024 / 1024
            )));
        }
    }

    fn push(&mut self, name: &str) -> Range<u32> {
        if self.text.len() + name.len() > MAX_NAMES {
            self.is_full = true;
            return 0..0;
        }
        let start = self.text.len() as u32;
        self.text.push_str(name);
        start..self.text.len() as u32
    }
}

fn kind_of(kind: u8) -> SymbolKind {
    match kind {
        elf::STT_NOTYPE => SymbolKind::None,
        elf::STT_OBJECT => SymbolKind::Object,
        elf::STT_FUNC => SymbolKind::Func,
        elf::STT_COMMON => SymbolKind::Common,
        elf::STT_TLS => SymbolKind::Tls,
        other => SymbolKind::Other(other),
    }
}

fn bind_of(bind: u8) -> Bind {
    match bind {
        elf::STB_LOCAL => Bind::Local,
        elf::STB_GLOBAL => Bind::Global,
        elf::STB_WEAK => Bind::Weak,
        other => Bind::Other(other),
    }
}

fn place_of(index: u16, sections: usize) -> Place {
    match index {
        elf::SHN_UNDEF => Place::Undefined,
        elf::SHN_ABS => Place::Absolute,
        elf::SHN_COMMON => Place::Common,
        index if usize::from(index) < sections && index < elf::SHN_LORESERVE => {
            Place::Section(u32::from(index))
        }
        other => Place::Other(other),
    }
}

fn find_table<S: SectionHeader<Endian = Endianness>>(
    headers: &[S],
    endian: Endianness,
) -> Option<&S> {
    headers
        .iter()
        .find(|section| section.sh_type(endian) == elf::SHT_SYMTAB)
        .or_else(|| {
            headers
                .iter()
                .find(|section| section.sh_type(endian) == elf::SHT_DYNSYM)
        })
}

fn entries<T: Pod>(bytes: &[u8]) -> &[T] {
    let count = bytes.len() / size_of::<T>();
    object::pod::slice_from_bytes(bytes, count).map_or(&[], |(entries, _)| entries)
}

type Located<'a> = (&'a [u8], &'a [u8], u64);

fn locate<'a, S, R>(
    table: &SymbolTable<'a, S, R>,
    problems: &mut Vec<ElfProblem>,
) -> Option<Located<'a>>
where
    S: SectionHeader<Endian = Endianness>,
    R: ReadCacheOps,
{
    let endian = table.endian;
    let Some(symtab) = find_table(table.headers, endian) else {
        problems.push(ElfProblem::Stripped);
        return None;
    };
    if symtab.sh_type(endian) != elf::SHT_SYMTAB {
        problems.push(ElfProblem::Stripped);
    }
    let found = bytes_of((symtab, endian), table.data, "symbol table").and_then(|bytes| {
        let strtab = table
            .headers
            .get(symtab.sh_link(endian) as usize)
            .ok_or_else(|| ElfProblem::Malformed("symbol names: no string table".to_owned()))?;
        Ok((
            bytes,
            bytes_of((strtab, endian), table.data, "symbol names")?,
        ))
    });
    match found {
        Ok((bytes, strings)) => Some((bytes, strings, symtab.sh_offset(endian).into())),
        Err(problem) => {
            problems.push(problem);
            None
        }
    }
}

fn symbol_of<Y: Sym<Endian = Endianness>>(
    sym: &Y,
    (strings, endian, sections): (&[u8], Endianness, usize),
    names: &mut Names,
) -> Symbol {
    let name = name_at(strings, sym.st_name(endian)).unwrap_or_else(|| {
        names.bad += 1;
        String::new()
    });
    let (name, raw) = match demangle(&name) {
        Some(shown) => (names.push(&shown), Some(names.push(&name))),
        None => (names.push(&name), None),
    };
    Symbol {
        name,
        raw,
        value: sym.st_value(endian).into(),
        size: sym.st_size(endian).into(),
        kind: kind_of(sym.st_type()),
        bind: bind_of(sym.st_bind()),
        place: place_of(sym.st_shndx(endian), sections),
    }
}

pub(super) fn read_symbols<S, R>(
    table: &SymbolTable<'_, S, R>,
    watch: &mut Watch<'_>,
    problems: &mut Vec<ElfProblem>,
) -> Result<(Vec<Symbol>, String), IndexError>
where
    S: SectionHeader<Endian = Endianness>,
    S::Elf: object::read::elf::FileHeader<Endian = Endianness>,
    R: ReadCacheOps,
{
    let Some((bytes, strings, start)) = locate(table, problems) else {
        return Ok((Vec::new(), String::new()));
    };
    let raw: &[<S::Elf as object::read::elf::FileHeader>::Sym] = entries(bytes);
    let total = raw.len().saturating_sub(1);
    if total > MAX_SYMBOLS {
        problems.push(ElfProblem::TooMany {
            what: "symbols",
            kept: MAX_SYMBOLS as u64,
            total: total as u64,
        });
    }
    let mut names = Names::default();
    let mut symbols = Vec::with_capacity(total.min(MAX_SYMBOLS));
    let context = (strings, table.endian, table.sections.len());
    for (at, sym) in raw.iter().enumerate().skip(1).take(MAX_SYMBOLS) {
        if at % CHECK_EVERY == 0 {
            if watch.cancel.load(Ordering::Relaxed) {
                return Err(IndexError::Cancelled);
            }
            (watch.progress)(start + (at * size_of_val(sym)) as u64);
        }
        if !matches!(sym.st_type(), elf::STT_SECTION | elf::STT_FILE) {
            symbols.push(symbol_of(sym, context, &mut names));
        }
    }
    names.report(problems);
    Ok((symbols, names.text))
}
