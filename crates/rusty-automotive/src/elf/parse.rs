use object::elf;
use object::read::ReadCacheOps;
use object::read::elf::{FileHeader, ProgramHeader, SectionHeader};
use object::{Endian as _, Endianness, FileKind, ReadCache, ReadRef};

use super::symbols::{SymbolTable, read_symbols};
use super::{Class, Elf, ElfProblem, Endian, Header, Section, Segment, Totals};
use crate::{IndexError, Watch};

const MAX_SECTIONS: usize = 65_536;
const MAX_SEGMENTS: usize = 4_096;
pub(super) const MAX_TABLE: u64 = 256 * 1024 * 1024;
pub(super) const MAX_NAME: usize = 1024;

const DWARF: [&str; 10] = [
    ".debug_abbrev",
    ".debug_addr",
    ".debug_info",
    ".debug_line",
    ".debug_line_str",
    ".debug_ranges",
    ".debug_rnglists",
    ".debug_str",
    ".debug_str_offsets",
    ".debug_aranges",
];

pub(super) type Data<'a, R> = &'a ReadCache<R>;

pub(super) fn parse<R: ReadCacheOps>(
    data: Data<'_, R>,
    watch: &mut Watch<'_>,
) -> Result<Elf, IndexError> {
    let kind = FileKind::parse(data).map_err(|error| IndexError::Malformed(error.to_string()))?;
    match kind {
        FileKind::Elf32 => parse_as::<elf::FileHeader32<Endianness>, R>(data, watch),
        FileKind::Elf64 => parse_as::<elf::FileHeader64<Endianness>, R>(data, watch),
        _ => Err(IndexError::Malformed("not an ELF file".to_owned())),
    }
}

pub(super) fn name_at(table: &[u8], offset: u32) -> Option<String> {
    let rest = table.get(offset as usize..)?;
    let end = rest
        .iter()
        .take(MAX_NAME)
        .position(|byte| *byte == 0)
        .unwrap_or(rest.len().min(MAX_NAME));
    let name = rest.get(..end)?;
    Some(String::from_utf8_lossy(name).into_owned())
}

pub(super) fn bytes_of<'a, S: SectionHeader<Endian = Endianness>, R: ReadCacheOps>(
    (section, endian): (&S, Endianness),
    data: Data<'a, R>,
    what: &str,
) -> Result<&'a [u8], ElfProblem> {
    let Some((offset, size)) = section.file_range(endian) else {
        return Ok(&[]);
    };
    if size > MAX_TABLE {
        return Err(ElfProblem::Malformed(format!(
            "{what} is larger than 256 MB"
        )));
    }
    data.read_bytes_at(offset, size)
        .map_err(|()| ElfProblem::Malformed(format!("{what} lies outside the file")))
}

fn header_of<Elf: FileHeader<Endian = Endianness>>(header: &Elf, endian: Endianness) -> Header {
    Header {
        class: if header.is_class_64() {
            Class::Elf64
        } else {
            Class::Elf32
        },
        endian: if endian.is_big_endian() {
            Endian::Big
        } else {
            Endian::Little
        },
        machine: header.e_machine(endian),
        kind: header.e_type(endian),
        entry: header.e_entry(endian).into(),
        flags: header.e_flags(endian),
    }
}

fn parse_as<Elf: FileHeader<Endian = Endianness>, R: ReadCacheOps>(
    data: Data<'_, R>,
    watch: &mut Watch<'_>,
) -> Result<super::Elf, IndexError> {
    let header = Elf::parse(data).map_err(|error| IndexError::Malformed(error.to_string()))?;
    let endian = header
        .endian()
        .map_err(|error| IndexError::Malformed(error.to_string()))?;
    let mut problems = Vec::new();
    let headers = header
        .section_headers(endian, data)
        .unwrap_or_else(|error| {
            problems.push(ElfProblem::Malformed(format!("section headers: {error}")));
            &[]
        });
    let headers = capped(headers, MAX_SECTIONS, "sections", &mut problems);
    let sections = sections_of::<Elf, R>((header, endian), headers, data, &mut problems);
    let segments = segments_of::<Elf, R>((header, endian), data, &mut problems);
    let table = SymbolTable {
        endian,
        headers,
        sections: &sections,
        data,
    };
    let (symbols, names) = read_symbols(&table, watch, &mut problems)?;
    let dwarf = dwarf_of(&sections, &mut problems);
    let totals = totals(&sections, &segments);
    Ok(super::Elf {
        header: header_of(header, endian),
        sections,
        segments,
        symbols,
        problems,
        totals,
        names,
        dwarf,
    })
}

fn capped<'a, T>(
    all: &'a [T],
    max: usize,
    what: &'static str,
    problems: &mut Vec<ElfProblem>,
) -> &'a [T] {
    if all.len() > max {
        problems.push(ElfProblem::TooMany {
            what,
            kept: max as u64,
            total: all.len() as u64,
        });
    }
    all.get(..max).unwrap_or(all)
}

fn sections_of<Elf: FileHeader<Endian = Endianness>, R: ReadCacheOps>(
    (header, endian): (&Elf, Endianness),
    headers: &[Elf::SectionHeader],
    data: Data<'_, R>,
    problems: &mut Vec<ElfProblem>,
) -> Vec<Section> {
    let strings = match header.shstrndx(endian, data) {
        Ok(index) => headers.get(index as usize).map_or(Ok(&[][..]), |table| {
            bytes_of((table, endian), data, "section names")
        }),
        Err(_) if headers.is_empty() => Ok(&[][..]),
        Err(error) => Err(ElfProblem::Malformed(format!("section names: {error}"))),
    };
    let strings = strings.unwrap_or_else(|problem| {
        problems.push(problem);
        &[]
    });
    headers
        .iter()
        .map(|section| Section {
            name: name_at(strings, section.sh_name(endian)).unwrap_or_default(),
            kind: section.sh_type(endian),
            flags: section.sh_flags(endian).into(),
            address: section.sh_addr(endian).into(),
            size: section.sh_size(endian).into(),
            offset: section.sh_offset(endian).into(),
        })
        .collect()
}

fn segments_of<Elf: FileHeader<Endian = Endianness>, R: ReadCacheOps>(
    (header, endian): (&Elf, Endianness),
    data: Data<'_, R>,
    problems: &mut Vec<ElfProblem>,
) -> Vec<Segment> {
    let headers = header
        .program_headers(endian, data)
        .unwrap_or_else(|error| {
            problems.push(ElfProblem::Malformed(format!("program headers: {error}")));
            &[]
        });
    capped(headers, MAX_SEGMENTS, "segments", problems)
        .iter()
        .map(|segment| Segment {
            kind: segment.p_type(endian),
            flags: segment.p_flags(endian),
            offset: segment.p_offset(endian).into(),
            virtual_address: segment.p_vaddr(endian).into(),
            physical_address: segment.p_paddr(endian).into(),
            file_size: segment.p_filesz(endian).into(),
            memory_size: segment.p_memsz(endian).into(),
        })
        .collect()
}

fn dwarf_of(sections: &[Section], problems: &mut Vec<ElfProblem>) -> Vec<(&'static str, usize)> {
    let is_compressed = sections.iter().any(|section| {
        section.name.starts_with(".zdebug_")
            || (section.name.starts_with(".debug_")
                && section.flags & u64::from(elf::SHF_COMPRESSED) != 0)
    });
    let found: Vec<(&'static str, usize)> = DWARF
        .iter()
        .filter_map(|name| {
            let at = sections.iter().position(|section| {
                section.name == *name
                    && section.kind != elf::SHT_NOBITS
                    && section.flags & u64::from(elf::SHF_COMPRESSED) == 0
            })?;
            Some((*name, at))
        })
        .collect();
    if is_compressed {
        problems.push(ElfProblem::CompressedDwarf);
    } else if !found.iter().any(|(name, _)| *name == ".debug_line") {
        problems.push(ElfProblem::NoDwarf);
    }
    found
}

fn totals(sections: &[Section], segments: &[Segment]) -> Totals {
    let loads: Vec<&Segment> = segments
        .iter()
        .filter(|segment| segment.is_load())
        .collect();
    if !loads.is_empty() {
        let load = loads
            .iter()
            .fold(0u64, |sum, segment| sum.saturating_add(segment.file_size));
        let ram = loads
            .iter()
            .filter(|segment| segment.is_writable())
            .fold(0u64, |sum, segment| sum.saturating_add(segment.memory_size));
        return Totals {
            load,
            ram,
            from_segments: true,
        };
    }
    let alloc = u64::from(elf::SHF_ALLOC);
    let write = u64::from(elf::SHF_WRITE);
    let mut totals = Totals::default();
    for section in sections.iter().filter(|section| section.flags & alloc != 0) {
        if section.kind != elf::SHT_NOBITS {
            totals.load = totals.load.saturating_add(section.size);
        }
        if section.flags & write != 0 {
            totals.ram = totals.ram.saturating_add(section.size);
        }
    }
    totals
}
