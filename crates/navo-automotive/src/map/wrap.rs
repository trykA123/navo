use super::MAX_MAP_LINE;
use super::parts::Part;
use super::table::Table;

pub(super) struct Wrapped {
    cells: Vec<String>,
    filled: Vec<bool>,
    pub line: u64,
}

fn is_filled(raw: &str) -> bool {
    let tail = raw.trim_end_matches(char::is_whitespace);
    !tail.is_empty() && raw.len() - tail.len() <= 1
}

pub(super) fn mandatory(part: Part, table: &Table) -> Vec<bool> {
    table
        .columns
        .iter()
        .map(|column| {
            let name = column.name.as_str();
            let size = name.contains("size");
            let place = name.contains("addr") || name.contains("offset");
            match part {
                Part::Locate => size || name.starts_with("chip"),
                Part::Removed => matches!(name, "file" | "object file"),
                Part::Symbols => place || name == "value",
                Part::Link | Part::Memory | Part::Skip => size || place,
            }
        })
        .collect()
}

impl Wrapped {
    pub(super) fn start(raw: &[&str], line: u64) -> Self {
        Self {
            cells: raw.iter().map(|cell| cell.trim().to_owned()).collect(),
            filled: raw.iter().map(|cell| is_filled(cell)).collect(),
            line,
        }
    }

    pub(super) fn extend(&mut self, raw: &[&str], required: &[bool]) -> bool {
        let continues = raw.len() == self.cells.len()
            && raw.iter().any(|cell| !cell.trim().is_empty())
            && raw
                .iter()
                .zip(required)
                .any(|(cell, &required)| required && cell.trim().is_empty())
            && raw
                .iter()
                .zip(&self.cells)
                .all(|(cell, held)| held.len() + cell.trim().len() <= MAX_MAP_LINE);
        if !continues {
            return false;
        }
        for ((held, cell), filled) in self.cells.iter_mut().zip(raw).zip(&mut self.filled) {
            let text = cell.trim();
            if text.is_empty() {
                continue;
            }
            if !held.is_empty() && !*filled {
                held.push(' ');
            }
            held.push_str(text);
            *filled = is_filled(cell);
        }
        true
    }

    pub(super) fn cells(&self) -> impl Iterator<Item = &str> {
        self.cells.iter().map(String::as_str)
    }
}
