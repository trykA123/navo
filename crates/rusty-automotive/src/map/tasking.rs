use super::model::MapModel;
use super::parts::{self, Part};
use super::retention::{Record, Retention};
use super::rows;
use super::table::{Parsed, Side, Table, raw_cells};
use super::wrap::{Wrapped, mandatory};

pub(super) struct Tasking {
    pub model: MapModel,
    part: Part,
    table: Table,
    space: Option<String>,
    symbol_space: Option<String>,
    previous_title: Option<String>,
    file: String,
    unsupported_mau: bool,
    symbol_order: Option<String>,
    retention: Retention,
    wrapped: Option<Wrapped>,
    required: Vec<bool>,
}

impl Tasking {
    pub(super) fn new() -> Self {
        Self {
            model: MapModel::new(),
            part: Part::Skip,
            table: Table::default(),
            space: None,
            symbol_space: None,
            previous_title: None,
            file: String::new(),
            unsupported_mau: false,
            symbol_order: None,
            retention: Retention::default(),
            wrapped: None,
            required: Vec::new(),
        }
    }

    fn header(&mut self, name: String) {
        self.part = parts::part(&name);
        if self.part == Part::Skip
            && self.model.skipped_parts.len() < 32
            && !self
                .model
                .skipped_parts
                .iter()
                .any(|title| title.eq_ignore_ascii_case(&name))
        {
            self.model.skipped_parts.push(name.clone());
        }
        if self.part == Part::Symbols {
            let name = name.to_ascii_lowercase();
            if self.model.symbols.is_empty() {
                self.symbol_order = Some(name);
            } else if self.symbol_order.as_deref() != Some(name.as_str()) {
                self.part = Part::Skip;
            }
        }
        self.table = Table::default();
        self.space = None;
        self.symbol_space = None;
        self.file.clear();
        self.unsupported_mau = false;
    }

    fn address_space(&mut self, text: &str) {
        let (space, mau) = text.split_once(" (MAU = ").unwrap_or((text, "8bit)"));
        self.space = Some(space.trim().to_owned());
        self.symbol_space = None;
        self.table = Table::default();
        self.file.clear();
        self.unsupported_mau = mau.trim() != "8bit)";
        if self.unsupported_mau {
            self.model
                .malformed("only 8-bit minimum addressable units are supported");
        }
    }

    pub(super) fn clear_file(&mut self) {
        self.file.clear();
    }

    pub(super) fn drop_row(&mut self, reason: &'static str) {
        self.flush_row();
        self.clear_file();
        self.previous_title = None;
        self.model.malformed(reason);
    }

    pub(super) fn take(&mut self, text: &str) {
        let text = text.trim().trim_start_matches('\u{feff}');
        if self.continue_row(text) {
            return;
        }
        self.flush_row();
        let header = parts::header(text, self.previous_title.take().as_deref());
        if let Some(header) = header {
            return self.header(header);
        }
        self.previous_title = parts::title(text).map(str::to_owned);
        if self.previous_title.is_some() {
            return;
        }
        if self.part == Part::Skip {
            return;
        }
        if let Some(space) = text.strip_prefix("+ Space ") {
            return self.address_space(space);
        }
        if self.unsupported_mau
            || text.is_empty()
            || text.bytes().all(|byte| b"|+-= \t".contains(&byte))
        {
            return;
        }
        if !text.starts_with('|') {
            if !self.table.columns.is_empty() && !text.starts_with('+') {
                self.drop_row("expected a table row with pipe separators");
            }
            return;
        }
        self.take_row(text);
    }

    fn continue_row(&mut self, text: &str) -> bool {
        let Some(row) = self.wrapped.as_mut() else {
            return false;
        };
        let is_row = text.starts_with('|') && !text.bytes().all(|byte| b"|+-= \t".contains(&byte));
        is_row && row.extend(&raw_cells(text), &self.required)
    }

    pub(super) fn flush_row(&mut self) {
        let Some(row) = self.wrapped.take() else {
            return;
        };
        let cells: Vec<&str> = row.cells().collect();
        let now = std::mem::replace(&mut self.model.lines, row.line);
        self.store_row(&cells);
        self.model.lines = now;
    }

    fn take_row(&mut self, text: &str) {
        let raw = raw_cells(text);
        let cells: Vec<&str> = raw.iter().map(|cell| cell.trim()).collect();
        if self.start_table(&cells) {
            return;
        }
        let can_wrap = raw.len() == self.table.columns.len()
            && !self.retention.capped(self.part)
            && self.required.contains(&true);
        if can_wrap {
            self.wrapped = Some(Wrapped::start(&raw, self.model.lines));
            return;
        }
        self.store_row(&cells);
    }

    fn is_header(&self, table: &Table) -> bool {
        match self.part {
            Part::Locate => {
                table.has(&["section", "section name"])
                    && table.has(&["space addr", "start address", "address"])
            }
            Part::Symbols => {
                table.has(&["symbol", "name"])
                    && table.has(&["address", "value", "space addr", "chip addr"])
            }
            Part::Memory => table.has(&["memory"]) && table.has(&["total", "length"]),
            Part::Link => {
                (table.has(&["file"]) || table.find_side(&["file"], Some(Side::In)).is_some())
                    && (table.has(&["offset"])
                        || table.find_side(&["offset"], Some(Side::Out)).is_some())
            }
            Part::Removed => table.has(&["section"]) && table.has(&["file", "reason"]),
            Part::Skip => false,
        }
    }

    fn start_table(&mut self, cells: &[&str]) -> bool {
        if cells.iter().any(|cell| {
            [
                "section",
                "section name",
                "name",
                "symbol",
                "memory",
                "file",
                "space",
                "stack name",
                "[in] file",
                "in file",
            ]
            .iter()
            .any(|name| cell.eq_ignore_ascii_case(name))
        }) {
            let table = Table::from_cells(cells);
            if self.is_header(&table) {
                self.required = mandatory(self.part, &table);
                self.table = table;
                self.symbol_space = None;
                self.clear_file();
                return true;
            }
            if table.has(&["space", "stack name"]) && self.part == Part::Memory {
                self.table = Table::default();
                return true;
            }
        }
        false
    }

    fn store_row(&mut self, cells: &[&str]) {
        if self.table.columns.is_empty() || self.retention.capped(self.part) {
            self.clear_file();
            return;
        }
        match self.parse_row(cells) {
            Ok(Some(row)) => {
                if !self.retention.store(&mut self.model, row) {
                    self.clear_file();
                }
            }
            Ok(None) => {}
            Err(reason) => self.drop_row(reason),
        }
    }

    fn parse_row(&mut self, cells: &[&str]) -> Parsed<Option<Record>> {
        let row = self.table.row(cells)?;
        Ok(Some(match self.part {
            Part::Locate => Record::Section(rows::section(&row, self.space.as_deref())?),
            Part::Memory => {
                if row
                    .get(&["memory"])
                    .is_some_and(|name| name.eq_ignore_ascii_case("total"))
                {
                    return Ok(None);
                }
                let (region, consistent) = rows::memory(&row)?;
                if !consistent {
                    self.model
                        .malformed("memory code, data and reserved do not match total minus free");
                }
                Record::Memory(region)
            }
            Part::Symbols => {
                let space = self.symbol_space.as_deref().or(self.space.as_deref());
                let symbol = rows::symbol(&row, space)?;
                self.symbol_space.clone_from(&symbol.address_space);
                Record::Symbol(symbol)
            }
            Part::Removed => Record::Discarded(rows::discarded(&row)?),
            Part::Link => {
                let file_column = row
                    .table
                    .find_side(&["file"], Some(Side::In))
                    .or_else(|| row.table.find(&["file"]));
                if let Some(file) = file_column.and_then(|at| row.get_at(at)) {
                    self.file = file.to_owned();
                }
                Record::Contribution(rows::contribution(&row, &self.file)?)
            }
            Part::Skip => return Err("the table is not supported"),
        }))
    }
}
