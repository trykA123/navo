use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use gimli::{
    AttributeValue, Dwarf, EndianSlice, LineProgramHeader, RunTimeEndian, SectionId, Unit,
};
use object::read::ReadCacheOps;
use object::{ReadCache, ReadRef};

use super::parse::MAX_TABLE;
use super::{Elf, ElfProblem, Endian, Place, Section, SymbolKind};
use crate::{IndexError, Watch};

const MAX_ROWS_READ: u64 = 50_000_000;
const MAX_ROWS_KEPT: usize = 4_000_000;
const MAX_PROBLEMS: usize = 8;
const CHECK_EVERY: u64 = 65_536;
const LOOK_BACK: usize = 64;
const PER_MILLE: u64 = 1000;
const TOMBSTONES: [u64; 4] = [u64::MAX, u64::MAX - 1, 0xFFFF_FFFF, 0xFFFF_FFFE];

type Slice<'a> = EndianSlice<'a, RunTimeEndian>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLine {
    pub file: u32,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    address: u64,
    source: SourceLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Sequence {
    start: u64,
    end: u64,
    rows: Range<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines {
    pub files: Vec<String>,
    pub of_symbol: Vec<Option<SourceLine>>,
    pub problems: Vec<ElfProblem>,
    rows: Vec<Row>,
    sequences: Vec<Sequence>,
}

impl Lines {
    #[must_use]
    pub fn get(&self, symbol: usize) -> Option<(&str, u32)> {
        let found = (*self.of_symbol.get(symbol)?)?;
        self.named(found)
    }

    #[must_use]
    pub fn at(&self, address: u64) -> Option<(&str, u32)> {
        self.named(self.locate(address)?)
    }

    #[must_use]
    pub fn found(&self) -> usize {
        self.of_symbol.iter().filter(|line| line.is_some()).count()
    }

    fn named(&self, found: SourceLine) -> Option<(&str, u32)> {
        let file = self.files.get(found.file as usize)?;
        Some((file.as_str(), found.line))
    }

    fn locate(&self, address: u64) -> Option<SourceLine> {
        let after = self
            .sequences
            .partition_point(|sequence| sequence.start <= address);
        let sequence = self
            .sequences
            .get(after.saturating_sub(LOOK_BACK)..after)?
            .iter()
            .rev()
            .find(|sequence| address < sequence.end)?;
        let rows = self.rows.get(sequence.rows.clone())?;
        let at = rows.partition_point(|row| row.address <= address);
        rows.get(at.checked_sub(1)?).map(|row| row.source)
    }

    fn push_problem(&mut self, problem: ElfProblem) {
        if self.problems.len() < MAX_PROBLEMS && !self.problems.contains(&problem) {
            self.problems.push(problem);
        }
    }
}

fn executable(elf: &Elf) -> Vec<Range<u64>> {
    elf.sections
        .iter()
        .filter(|section| section.is_executable())
        .map(Section::span)
        .collect()
}

fn symbol_address(elf: &Elf, at: usize) -> Option<u64> {
    let symbol = elf.symbols.get(at)?;
    let section = match symbol.place {
        Place::Section(index) => elf.sections.get(index as usize)?,
        _ => return None,
    };
    if !section.is_executable() {
        return None;
    }
    let thumb = elf.header.machine == object::elf::EM_ARM && symbol.kind == SymbolKind::Func;
    Some(if thumb {
        symbol.value & !1
    } else {
        symbol.value
    })
}

fn dwarf_section<'a>(elf: &'a Elf, name: &str) -> Option<&'a Section> {
    elf.dwarf
        .iter()
        .find(|(known, _)| *known == name)
        .and_then(|(_, at)| elf.sections.get(*at))
}

fn load<'a, R: ReadCacheOps>(data: &'a ReadCache<R>, elf: &Elf, id: SectionId) -> Slice<'a> {
    let endian = match elf.header.endian {
        Endian::Little => RunTimeEndian::Little,
        Endian::Big => RunTimeEndian::Big,
    };
    let bytes = dwarf_section(elf, id.name())
        .filter(|section| section.size <= MAX_TABLE)
        .and_then(|section| data.read_bytes_at(section.offset, section.size).ok())
        .unwrap_or(&[]);
    EndianSlice::new(bytes, endian)
}

fn check_sizes<R: ReadCacheOps>(data: &ReadCache<R>, elf: &Elf, lines: &mut Lines) {
    for (name, at) in &elf.dwarf {
        let Some(section) = elf.sections.get(*at) else {
            continue;
        };
        if section.size > MAX_TABLE {
            lines.push_problem(ElfProblem::Malformed(format!(
                "{name} is larger than 256 MB and is not read"
            )));
        } else if data.read_bytes_at(section.offset, section.size).is_err() {
            lines.push_problem(ElfProblem::Malformed(format!(
                "{name} lies outside the file"
            )));
        }
    }
}

pub(super) fn read<R: ReadCacheOps>(
    data: &ReadCache<R>,
    elf: &Elf,
    watch: &mut Watch<'_>,
) -> Result<Lines, IndexError> {
    let mut lines = Lines {
        of_symbol: vec![None; elf.symbols.len()],
        ..Lines::default()
    };
    if !elf.has_dwarf() {
        return Ok(lines);
    }
    check_sizes(data, elf, &mut lines);
    let dwarf = Dwarf::load(|id| Ok::<_, gimli::Error>(load(data, elf, id)))
        .map_err(|error| IndexError::Malformed(error.to_string()))?;
    let info = dwarf_section(elf, ".debug_info").map_or(1, |section| section.size.max(1));
    let mut walk = Walk {
        lines,
        executable: executable(elf),
        paths: HashMap::new(),
        unit_files: HashMap::new(),
        pending: Vec::new(),
        rows_read: 0,
        is_cancelled: false,
        is_full: false,
        cancel: watch.cancel,
        dwarf: &dwarf,
    };
    let mut units = dwarf.units();
    while !walk.is_done() {
        let header = match units.next() {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(error) => {
                walk.lines.push_problem(malformed(&error));
                break;
            }
        };
        let at = header
            .offset()
            .as_debug_info_offset()
            .map_or(0, |offset| offset.0 as u64);
        (watch.progress)(at * PER_MILLE / info);
        if let Err(error) = dwarf.unit(header).and_then(|unit| walk.unit(&unit)) {
            walk.lines.push_problem(malformed(&error));
        }
    }
    if walk.is_cancelled || watch.cancel.load(Ordering::Relaxed) {
        return Err(IndexError::Cancelled);
    }
    Ok(walk.finish(elf))
}

fn malformed(error: &gimli::Error) -> ElfProblem {
    ElfProblem::Malformed(format!("DWARF: {error}"))
}

fn line_of(row: &gimli::LineRow) -> u32 {
    row.line()
        .map_or(0, |line| u32::try_from(line.get()).unwrap_or(u32::MAX))
}

fn is_ambiguous(start: u64, sequences: &[Sequence]) -> bool {
    (start == 0 || TOMBSTONES.contains(&start))
        && sequences
            .iter()
            .filter(|other| other.start == start)
            .count()
            > 1
}

struct Walk<'a> {
    lines: Lines,
    executable: Vec<Range<u64>>,
    paths: HashMap<String, u32>,
    unit_files: HashMap<u64, u32>,
    pending: Vec<Row>,
    rows_read: u64,
    is_cancelled: bool,
    is_full: bool,
    cancel: &'a AtomicBool,
    dwarf: &'a Dwarf<Slice<'a>>,
}

impl<'a> Walk<'a> {
    fn is_done(&self) -> bool {
        self.is_cancelled || self.is_full
    }

    fn count_row(&mut self) {
        self.rows_read += 1;
        if self.rows_read.is_multiple_of(CHECK_EVERY) && self.cancel.load(Ordering::Relaxed) {
            self.is_cancelled = true;
        }
        if self.rows_read > MAX_ROWS_READ {
            self.is_full = true;
        }
    }

    fn unit(&mut self, unit: &Unit<Slice<'a>>) -> gimli::Result<()> {
        let Some(program) = unit.line_program.clone() else {
            return Ok(());
        };
        self.unit_files.clear();
        self.pending.clear();
        let mut rows = program.rows();
        while let Some((header, row)) = rows.next_row()? {
            self.count_row();
            if self.is_done() {
                return Ok(());
            }
            if row.end_sequence() {
                self.close_sequence(row.address());
            } else if self.pending.len() < MAX_ROWS_KEPT {
                let file = self.file_of(unit, header, row.file_index());
                self.pending.push(Row {
                    address: row.address(),
                    source: SourceLine {
                        file,
                        line: line_of(row),
                    },
                });
            }
        }
        Ok(())
    }

    fn file_of(
        &mut self,
        unit: &Unit<Slice<'a>>,
        header: &LineProgramHeader<Slice<'a>>,
        file: u64,
    ) -> u32 {
        if let Some(id) = self.unit_files.get(&file) {
            return *id;
        }
        let id = self.file_id(unit, header, file);
        self.unit_files.insert(file, id);
        id
    }

    fn is_wanted(&self, start: u64) -> bool {
        self.executable.is_empty() || self.executable.iter().any(|span| span.contains(&start))
    }

    fn close_sequence(&mut self, end: u64) {
        let mut rows = std::mem::take(&mut self.pending);
        let Some(start) = rows.first().map(|row| row.address) else {
            return;
        };
        if start >= end || !self.is_wanted(start) {
            return;
        }
        if self.lines.rows.len() + rows.len() > MAX_ROWS_KEPT {
            self.is_full = true;
            return;
        }
        rows.sort_by_key(|row| row.address);
        let first = self.lines.rows.len();
        self.lines.rows.extend(rows);
        self.lines.sequences.push(Sequence {
            start,
            end,
            rows: first..self.lines.rows.len(),
        });
    }

    fn finish(mut self, elf: &Elf) -> Lines {
        if self.is_full {
            self.lines.push_problem(ElfProblem::Stopped {
                what: "DWARF line rows",
                after: self.rows_read,
            });
        }
        let mut lines = self.lines;
        lines.sequences.sort_by_key(|sequence| sequence.start);
        let kept: Vec<Sequence> = lines
            .sequences
            .iter()
            .filter(|sequence| !is_ambiguous(sequence.start, &lines.sequences))
            .cloned()
            .collect();
        lines.sequences = kept;
        lines.of_symbol = (0..elf.symbols.len())
            .map(|at| symbol_address(elf, at).and_then(|address| lines.locate(address)))
            .collect();
        lines
    }

    fn text(&self, unit: &Unit<Slice<'a>>, value: AttributeValue<Slice<'a>>) -> String {
        self.dwarf
            .attr_string(unit, value)
            .map(|text| text.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn directory(
        &self,
        unit: &Unit<Slice<'a>>,
        header: &LineProgramHeader<Slice<'a>>,
        entry: &gimli::FileEntry<Slice<'a>>,
    ) -> String {
        let directory = entry
            .directory(header)
            .map(|directory| self.text(unit, directory))
            .unwrap_or_default();
        let comp = unit
            .comp_dir
            .map(|comp| comp.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_absolute(&directory) || comp.is_empty() || directory == comp {
            directory
        } else if directory.is_empty() {
            comp
        } else {
            join(&comp, &directory)
        }
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
                join(&self.directory(unit, header, entry), &name)
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

fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || path.starts_with('\\') || path.as_bytes().get(1) == Some(&b':')
}

fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() || is_absolute(name) {
        return name.to_owned();
    }
    let separator = if directory.contains('\\') { '\\' } else { '/' };
    let directory = directory.trim_end_matches(['/', '\\']);
    format!("{directory}{separator}{name}")
}
