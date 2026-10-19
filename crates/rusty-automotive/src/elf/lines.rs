use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::Ordering;

use gimli::{
    AttributeValue, Dwarf, EndianSlice, LineProgramHeader, RunTimeEndian, SectionId, Unit,
};
use object::read::ReadCacheOps;
use object::{ReadCache, ReadRef};

use super::parse::MAX_TABLE;
use super::{Elf, ElfProblem, Endian, Place, SymbolKind};
use crate::{IndexError, Watch};

const MAX_ROWS: u64 = 50_000_000;
const MAX_VISITS: u64 = 50_000_000;
const PER_MILLE: u64 = 1000;

type Slice<'a> = EndianSlice<'a, RunTimeEndian>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLine {
    pub file: u32,
    pub line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines {
    pub files: Vec<String>,
    pub of_symbol: Vec<Option<SourceLine>>,
    pub problem: Option<ElfProblem>,
}

impl Lines {
    #[must_use]
    pub fn get(&self, symbol: usize) -> Option<(&str, u32)> {
        let found = (*self.of_symbol.get(symbol)?)?;
        let file = self.files.get(found.file as usize)?;
        Some((file.as_str(), found.line))
    }

    #[must_use]
    pub fn found(&self) -> usize {
        self.of_symbol.iter().filter(|line| line.is_some()).count()
    }
}

struct Walk<'a> {
    wanted: Vec<(u64, u32)>,
    lines: Lines,
    paths: HashMap<String, u32>,
    unit_files: HashMap<u64, u32>,
    rows: u64,
    visits: u64,
    dwarf: &'a Dwarf<Slice<'a>>,
}

fn wanted(elf: &Elf) -> Vec<(u64, u32)> {
    let is_arm = elf.header.machine == object::elf::EM_ARM;
    let mut wanted: Vec<(u64, u32)> = elf
        .symbols
        .iter()
        .enumerate()
        .filter(|(_, symbol)| matches!(symbol.place, Place::Section(_)))
        .map(|(at, symbol)| {
            let thumb = is_arm && symbol.kind == SymbolKind::Func;
            let address = if thumb {
                symbol.value & !1
            } else {
                symbol.value
            };
            (address, at as u32)
        })
        .collect();
    wanted.sort_unstable();
    wanted
}

fn load<'a, R: ReadCacheOps>(data: &'a ReadCache<R>, elf: &Elf, id: SectionId) -> Slice<'a> {
    let endian = match elf.header.endian {
        Endian::Little => RunTimeEndian::Little,
        Endian::Big => RunTimeEndian::Big,
    };
    let bytes = elf
        .dwarf
        .iter()
        .find(|(name, _)| *name == id.name())
        .and_then(|(_, at)| elf.sections.get(*at))
        .filter(|section| section.size <= MAX_TABLE)
        .and_then(|section| data.read_bytes_at(section.offset, section.size).ok())
        .unwrap_or(&[]);
    EndianSlice::new(bytes, endian)
}

pub(super) fn read<R: ReadCacheOps>(
    data: &ReadCache<R>,
    elf: &Elf,
    watch: &mut Watch<'_>,
) -> Result<Lines, IndexError> {
    let lines = Lines {
        of_symbol: vec![None; elf.symbols.len()],
        ..Lines::default()
    };
    if !elf.has_dwarf() || elf.symbols.is_empty() {
        return Ok(lines);
    }
    let dwarf = Dwarf::load(|id| Ok::<_, gimli::Error>(load(data, elf, id)))
        .map_err(|error| IndexError::Malformed(error.to_string()))?;
    let info = elf
        .dwarf
        .iter()
        .find(|(name, _)| *name == ".debug_info")
        .and_then(|(_, at)| elf.sections.get(*at))
        .map_or(1, |section| section.size.max(1));
    let mut walk = Walk {
        wanted: wanted(elf),
        lines,
        paths: HashMap::new(),
        unit_files: HashMap::new(),
        rows: 0,
        visits: 0,
        dwarf: &dwarf,
    };
    let mut units = dwarf.units();
    loop {
        if watch.cancel.load(Ordering::Relaxed) {
            return Err(IndexError::Cancelled);
        }
        let header = match units.next() {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(error) => {
                walk.lines.problem = Some(malformed(&error));
                break;
            }
        };
        let at = header
            .offset()
            .as_debug_info_offset()
            .map_or(0, |offset| offset.0 as u64);
        (watch.progress)(at * PER_MILLE / info);
        if !walk.step(header) {
            break;
        }
    }
    Ok(walk.lines)
}

fn malformed(error: &gimli::Error) -> ElfProblem {
    ElfProblem::Malformed(format!("DWARF: {error}"))
}

impl<'a> Walk<'a> {
    fn is_over(&self) -> bool {
        self.rows > MAX_ROWS || self.visits > MAX_VISITS
    }

    fn step(&mut self, header: gimli::UnitHeader<Slice<'a>>) -> bool {
        let step = self.dwarf.unit(header).and_then(|unit| self.unit(&unit));
        if let Err(error) = step {
            self.lines.problem = Some(malformed(&error));
        }
        if self.is_over() {
            self.lines.problem = Some(ElfProblem::TooMany {
                what: "DWARF line rows",
                kept: self.rows.min(MAX_ROWS),
                total: self.rows,
            });
            return false;
        }
        true
    }

    fn close(
        &mut self,
        (start, file, line): (u64, u64, u32),
        end: u64,
        (unit, header): (&Unit<Slice<'a>>, &LineProgramHeader<Slice<'a>>),
    ) {
        let found = self.assign(start..end);
        if found.is_empty() {
            return;
        }
        let file = match self.unit_files.get(&file) {
            Some(id) => *id,
            None => {
                let id = self.file_id(unit, header, file);
                self.unit_files.insert(file, id);
                id
            }
        };
        let line = SourceLine { file, line };
        for symbol in found {
            if let Some(slot) = self.lines.of_symbol.get_mut(symbol as usize) {
                *slot = Some(line);
            }
        }
    }

    fn unit(&mut self, unit: &Unit<Slice<'a>>) -> gimli::Result<()> {
        let Some(program) = unit.line_program.clone() else {
            return Ok(());
        };
        self.unit_files.clear();
        let mut rows = program.rows();
        let mut open: Option<(u64, u64, u32)> = None;
        while let Some((header, row)) = rows.next_row()? {
            self.rows += 1;
            if self.is_over() {
                return Ok(());
            }
            if let Some(open) = open {
                self.close(open, row.address(), (unit, header));
            }
            open = (!row.end_sequence()).then(|| {
                let line = row
                    .line()
                    .map_or(0, |line| u32::try_from(line.get()).unwrap_or(u32::MAX));
                (row.address(), row.file_index(), line)
            });
        }
        Ok(())
    }

    fn assign(&mut self, range: Range<u64>) -> Vec<u32> {
        let start = self
            .wanted
            .partition_point(|(address, _)| *address < range.start);
        let mut found = Vec::new();
        for (address, symbol) in self.wanted.iter().skip(start) {
            if *address >= range.end {
                break;
            }
            self.visits += 1;
            let is_free = self
                .lines
                .of_symbol
                .get(*symbol as usize)
                .is_some_and(Option::is_none);
            if is_free {
                found.push(*symbol);
            }
        }
        found
    }

    fn text(&self, unit: &Unit<Slice<'a>>, value: AttributeValue<Slice<'a>>) -> String {
        self.dwarf
            .attr_string(unit, value)
            .map(|text| text.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn file_id(
        &mut self,
        unit: &Unit<Slice<'a>>,
        header: &LineProgramHeader<Slice<'a>>,
        file: u64,
    ) -> u32 {
        let path = header.file(file).map_or_else(
            || "?".to_owned(),
            |entry| {
                let name = self.text(unit, entry.path_name());
                let directory = entry
                    .directory(header)
                    .map(|directory| self.text(unit, directory))
                    .unwrap_or_default();
                join(&directory, &name)
            },
        );
        let next = self.lines.files.len() as u32;
        let id = *self.paths.entry(path.clone()).or_insert(next);
        if id == next {
            self.lines.files.push(path);
        }
        id
    }
}

fn join(directory: &str, name: &str) -> String {
    let is_absolute =
        name.starts_with('/') || name.starts_with('\\') || name.as_bytes().get(1) == Some(&b':');
    if directory.is_empty() || is_absolute {
        return name.to_owned();
    }
    let separator = if directory.contains('\\') { '\\' } else { '/' };
    let directory = directory.trim_end_matches(['/', '\\']);
    format!("{directory}{separator}{name}")
}
