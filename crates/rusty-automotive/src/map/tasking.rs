use super::model::{MapModel, MapProblem};
use super::rows;
use super::table::{Parsed, Table, cells};

#[derive(Default, Clone, Copy)]
enum Part {
    #[default]
    Skip,
    Locate,
    Memory,
    Symbols,
    Link,
    Removed,
}

pub(super) struct Tasking {
    pub model: MapModel,
    part: Part,
    table: Table,
    space: Option<String>,
    file: String,
    unsupported_mau: bool,
    entries: usize,
    text_bytes: usize,
    capped: bool,
}

impl Tasking {
    pub(super) fn new() -> Self {
        Self {
            model: MapModel::new(),
            part: Part::Skip,
            table: Table::default(),
            space: None,
            file: String::new(),
            unsupported_mau: false,
            entries: 0,
            text_bytes: 0,
            capped: false,
        }
    }

    fn header(&mut self, header: &str) {
        let name = header.trim().trim_end_matches(':').to_ascii_lowercase();
        self.part = match name.as_str() {
            "locate result" | "locate result: sections" | "sections" => Part::Locate,
            "link result" => Part::Link,
            "removed sections" => Part::Removed,
            _ if name.starts_with("symbols") || name == "locate result: symbols" => Part::Symbols,
            _ if name.starts_with("memory usage") => Part::Memory,
            _ => {
                self.model.skipped_sections += 1;
                Part::Skip
            }
        };
        self.table = Table::default();
        self.space = None;
        self.file.clear();
        self.unsupported_mau = false;
    }

    fn address_space(&mut self, text: &str) {
        let (space, mau) = text.split_once(" (MAU = ").unwrap_or((text, "8bit)"));
        self.space = Some(space.trim().to_owned());
        self.table = Table::default();
        self.unsupported_mau = mau.trim() != "8bit)";
        if self.unsupported_mau {
            self.model
                .malformed("only 8-bit minimum addressable units are supported");
        }
    }

    pub(super) fn take(&mut self, raw: &[u8]) {
        let text = match std::str::from_utf8(raw) {
            Ok(text) => text.trim().trim_start_matches('\u{feff}'),
            Err(_) => return self.model.malformed("the line is not UTF-8"),
        };
        if let Some(header) = text.strip_prefix('*').map(str::trim)
            && !header.is_empty()
            && !header.starts_with('*')
        {
            return self.header(header);
        }
        if let Some(space) = text.strip_prefix("+ Space ") {
            return self.address_space(space);
        }
        if matches!(self.part, Part::Skip)
            || self.unsupported_mau
            || text.is_empty()
            || text.bytes().all(|byte| b"|+-= \t".contains(&byte))
        {
            return;
        }
        if !text.starts_with('|') {
            if !self.table.columns.is_empty() && !text.starts_with('+') {
                self.model
                    .malformed("expected a table row with pipe separators");
            }
            return;
        }
        self.take_cells(&cells(text));
    }

    fn is_header(&self, table: &Table) -> bool {
        match self.part {
            Part::Locate => {
                table.has(&["section", "section name"])
                    && table.has(&["space addr", "start address", "address"])
            }
            Part::Symbols => table.has(&["symbol", "name"]) && table.has(&["address", "value"]),
            Part::Memory => table.has(&["memory"]) && table.has(&["total", "length"]),
            Part::Link => {
                table.has(&["file", "[in] file", "in file"])
                    && table.has(&["offset", "[out] offset", "out offset"])
            }
            Part::Removed => table.has(&["section"]) && table.has(&["file", "reason"]),
            Part::Skip => false,
        }
    }

    fn take_cells(&mut self, cells: &[&str]) {
        if cells.iter().any(|cell| {
            [
                "address",
                "name",
                "section",
                "memory",
                "file",
                "space",
                "stack name",
                "[in] file",
                "symbol",
                "section name",
            ]
            .iter()
            .any(|name| cell.eq_ignore_ascii_case(name))
        }) {
            let table = Table::from_cells(cells);
            if self.is_header(&table) {
                self.table = table;
                return;
            }
            if table.has(&["space", "stack name"]) && matches!(self.part, Part::Memory) {
                self.table = Table::default();
                return;
            }
        }
        if self.table.columns.is_empty() || self.capped {
            return;
        }
        match self.parse_row(cells) {
            Ok(row) => self.store(row),
            Err(reason) => self.model.malformed(reason),
        }
    }

    fn parse_row(&mut self, cells: &[&str]) -> Parsed<Record> {
        let row = self.table.row(cells)?;
        Ok(match self.part {
            Part::Locate => Record::Section(rows::section(&row, self.space.as_deref())?),
            Part::Memory => Record::Memory(rows::memory(&row)?),
            Part::Symbols => Record::Symbol(rows::symbol(&row, self.space.as_deref())?),
            Part::Removed => Record::Discarded(rows::discarded(&row)?),
            Part::Link => {
                if let Some(file) = row.get(&["file", "[in] file", "in file"]) {
                    self.file = file.to_owned();
                }
                Record::Contribution(rows::contribution(&row, &self.file)?)
            }
            Part::Skip => return Err("the table is not supported"),
        })
    }

    fn store(&mut self, row: Record) {
        let text_bytes = row.text_bytes();
        let limit = if self.entries >= super::MAX_MAP_ENTRIES {
            Some("retained entries")
        } else if self.text_bytes + text_bytes > super::MAX_MAP_TEXT_BYTES {
            Some("retained text bytes")
        } else {
            None
        };
        if let Some(limit) = limit {
            self.capped = true;
            return self.model.note(MapProblem::CapReached {
                line: self.model.lines,
                limit,
            });
        }
        self.entries += 1;
        self.text_bytes += text_bytes;
        match row {
            Record::Section(row) => self.model.sections.push(row),
            Record::Symbol(row) => self.model.symbols.push(row),
            Record::Memory(row) => self.model.regions.push(row),
            Record::Contribution(row) => self.model.contributions.push(row),
            Record::Discarded(row) => self.model.discarded.push(row),
        }
    }
}

enum Record {
    Memory(super::MemoryRegion),
    Section(super::OutputSection),
    Symbol(super::Symbol),
    Contribution(super::Contribution),
    Discarded(super::DiscardedSection),
}

impl Record {
    fn text_bytes(&self) -> usize {
        let optional = |text: &Option<String>| text.as_ref().map_or(0, String::len);
        match self {
            Self::Memory(row) => row.name.len(),
            Self::Section(row) => {
                row.name.len() + optional(&row.region) + optional(&row.address_space)
            }
            Self::Symbol(row) => {
                row.name.len() + optional(&row.section) + optional(&row.address_space)
            }
            Self::Contribution(row) => {
                row.object_file.len() + row.section.len() + row.output_section.len()
            }
            Self::Discarded(row) => {
                row.name.len()
                    + optional(&row.object_file)
                    + optional(&row.library)
                    + optional(&row.symbol)
                    + optional(&row.reason)
            }
        }
    }
}
