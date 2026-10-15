mod records;
mod sniff;
mod sort;

pub use records::{RecordIndex, STRIDE, index_records, read_rows};
pub use sniff::{Dialect, sniff};
pub use sort::{is_numeric_column, sort_order};

use crate::ParseError;

const MIN_WIDTH: usize = 3;
const MAX_WIDTH: usize = 40;
const SORT_MARK: usize = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub columns: usize,
}

pub(crate) fn reader(dialect: Dialect, bytes: &[u8]) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new()
        .delimiter(dialect.delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(bytes)
}

pub(crate) fn record_cells(record: &csv::ByteRecord) -> Vec<String> {
    record
        .iter()
        .map(|cell| String::from_utf8_lossy(cell).into_owned())
        .collect()
}

pub(crate) fn csv_error(error: &csv::Error) -> ParseError {
    let (line, column) = error
        .position()
        .map_or((0, 0), |position| (position.line(), 1));
    ParseError::new(error.to_string(), line, column)
}

#[must_use]
pub fn column_name(index: usize) -> String {
    let mut name = String::new();
    let mut rest = index + 1;
    while rest > 0 {
        let digit = u8::try_from((rest - 1) % 26).unwrap_or(0);
        name.insert(0, char::from(b'A' + digit));
        rest = (rest - 1) / 26;
    }
    name
}

#[must_use]
pub fn header_of(first: Option<Vec<String>>, columns: usize) -> Vec<String> {
    let mut header = first.unwrap_or_default();
    let start = header.len();
    header.extend((start..columns).map(column_name));
    header
}

#[must_use]
pub fn first_record(head: &[u8], dialect: Dialect) -> Vec<String> {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let mut record = csv::ByteRecord::new();
    match reader(dialect, head).read_byte_record(&mut record) {
        Ok(true) => record_cells(&record),
        Ok(false) | Err(_) => Vec::new(),
    }
}

pub fn parse_table(bytes: &[u8], dialect: Dialect) -> Result<Table, ParseError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut rows = Vec::new();
    let mut record = csv::ByteRecord::new();
    let mut reader = reader(dialect, bytes);
    while reader
        .read_byte_record(&mut record)
        .map_err(|error| csv_error(&error))?
    {
        rows.push(record_cells(&record));
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let first = if dialect.has_header && !rows.is_empty() {
        Some(rows.remove(0))
    } else {
        None
    };
    Ok(Table {
        header: header_of(first, columns),
        rows,
        columns,
    })
}

#[must_use]
pub fn column_widths(header: &[String], rows: &[Vec<String>], columns: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = (0..columns)
        .map(|column| {
            header
                .get(column)
                .map_or(0, |cell| cell.chars().count() + SORT_MARK)
        })
        .collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    widths
        .into_iter()
        .map(|width| width.clamp(MIN_WIDTH, MAX_WIDTH))
        .collect()
}

#[cfg(test)]
mod tests;
