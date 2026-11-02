use super::{reader, record_cells};

const DELIMITERS: [u8; 4] = *b",;\t|";
const SAMPLE_RECORDS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialect {
    pub delimiter: u8,
    pub has_header: bool,
}

fn sample(head: &[u8], delimiter: u8) -> Vec<Vec<String>> {
    let dialect = Dialect {
        delimiter,
        has_header: false,
    };
    let mut rows = Vec::new();
    let mut record = csv::ByteRecord::new();
    let mut reader = reader(dialect, head);
    while rows.len() <= SAMPLE_RECORDS {
        match reader.read_byte_record(&mut record) {
            Ok(true) => rows.push(record_cells(&record)),
            Ok(false) | Err(_) => break,
        }
    }
    if rows.len() > 2 {
        rows.pop();
    }
    rows
}

fn consistency(rows: &[Vec<String>]) -> (usize, usize) {
    let mut counts: Vec<(usize, usize)> = Vec::new();
    for row in rows {
        match counts.iter_mut().find(|(fields, _)| *fields == row.len()) {
            Some((_, seen)) => *seen += 1,
            None => counts.push((row.len(), 1)),
        }
    }
    counts
        .into_iter()
        .filter(|(fields, _)| *fields > 1)
        .map(|(fields, seen)| (seen, fields))
        .max()
        .unwrap_or((0, 1))
}

pub(super) fn is_number(cell: &str) -> bool {
    let cell = cell.trim();
    !cell.is_empty() && cell.chars().any(|ch| ch.is_ascii_digit()) && cell.parse::<f64>().is_ok()
}

fn header_votes(rows: &[Vec<String>]) -> i32 {
    let Some((first, data)) = rows.split_first() else {
        return 0;
    };
    let mut votes = 0;
    for (column, title) in first.iter().enumerate() {
        let mut cells = data
            .iter()
            .filter_map(|row| row.get(column))
            .filter(|cell| !cell.trim().is_empty())
            .peekable();
        if cells.peek().is_none() || !cells.all(|cell| is_number(cell)) {
            continue;
        }
        votes += if is_number(title) { -1 } else { 1 };
    }
    votes
}

fn looks_like_titles(first: &[String]) -> bool {
    let unique = first
        .iter()
        .enumerate()
        .all(|(at, cell)| !first.iter().skip(at + 1).any(|other| other == cell));
    unique
        && first
            .iter()
            .all(|cell| !cell.trim().is_empty() && !is_number(cell))
}

fn has_header(rows: &[Vec<String>]) -> bool {
    let votes = header_votes(rows);
    if votes != 0 {
        return votes > 0;
    }
    rows.first().is_some_and(|first| looks_like_titles(first))
}

#[must_use]
pub fn sniff(head: &[u8]) -> Dialect {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let mut best = (0, 1);
    let mut chosen = (b',', Vec::new());
    for delimiter in DELIMITERS {
        let rows = sample(head, delimiter);
        let score = consistency(&rows);
        if score > best {
            best = score;
            chosen = (delimiter, rows);
        }
    }
    let (delimiter, rows) = if chosen.1.is_empty() {
        (b',', sample(head, b','))
    } else {
        chosen
    };
    Dialect {
        delimiter,
        has_header: has_header(&rows),
    }
}
