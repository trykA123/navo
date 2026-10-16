use std::borrow::Cow;

use pulldown_cmark::Options;

use super::{FOOTNOTES_AS_TEXT, TABLES_AS_TEXT};

const MAX_TABLE_CELLS: usize = 1_000_000;
const MAX_FOOTNOTE_LINES: usize = 2_000;

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    cells: usize,
    footnotes: usize,
}

fn delimiter_columns(line: &str) -> Option<usize> {
    let is_delimiter = line.contains('|')
        && line.contains('-')
        && line
            .chars()
            .all(|ch| matches!(ch, '|' | '-' | ':' | ' ' | '\t'));
    is_delimiter.then(|| {
        line.split('|')
            .filter(|cell| !cell.trim().is_empty())
            .count()
    })
}

fn without_prefix(line: &str) -> &str {
    line.trim_start_matches([' ', '\t', '>'])
}

fn escape_footnote_lines(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + source.len() / 8);
    for line in source.split_inclusive('\n') {
        let rest = without_prefix(line);
        if rest.starts_with("[^") {
            out.push_str(&line[..line.len() - rest.len()]);
            out.push('\\');
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
    }
    out
}

fn count(source: &str) -> Counts {
    let mut counts = Counts::default();
    let mut columns: Option<usize> = None;
    for line in source.lines() {
        let line = without_prefix(line).trim_end();
        if line.is_empty() {
            columns = None;
            continue;
        }
        if line.starts_with("[^") {
            counts.footnotes += 1;
        }
        if columns.is_none() {
            columns = delimiter_columns(line).filter(|columns| *columns > 0);
            let header = columns.unwrap_or(0);
            counts.cells = counts.cells.saturating_add(header);
        }
        counts.cells = counts.cells.saturating_add(columns.unwrap_or(0));
    }
    counts
}

pub(super) struct Plan<'a> {
    pub source: Cow<'a, str>,
    pub options: Options,
    pub notes: Vec<&'static str>,
}

pub(super) fn plan(source: &str) -> Plan<'_> {
    let counts = count(source);
    let mut plan = Plan {
        source: Cow::Borrowed(source),
        options: Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES,
        notes: Vec::new(),
    };
    if counts.cells > MAX_TABLE_CELLS {
        plan.options.remove(Options::ENABLE_TABLES);
        plan.notes.push(TABLES_AS_TEXT);
    }
    if counts.footnotes > MAX_FOOTNOTE_LINES {
        plan.options.remove(Options::ENABLE_FOOTNOTES);
        plan.source = Cow::Owned(escape_footnote_lines(source));
        plan.notes.push(FOOTNOTES_AS_TEXT);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::{Counts, count, escape_footnote_lines};

    #[test]
    fn cells_count_header_delimiter_and_rows_until_a_blank_line() {
        let table = "| a | b |\n|---|:-:|\n| 1 |\nx\n\nafter\n";
        assert_eq!(
            count(table),
            Counts {
                cells: 8,
                footnotes: 0
            }
        );
    }

    #[test]
    fn rules_and_setext_lines_are_not_tables() {
        assert_eq!(count("a\n---\n\n***\n- - -\n").cells, 0);
    }

    #[test]
    fn footnote_lines_count_after_spaces_and_quotes() {
        assert_eq!(count("[^a]: b\n > [^b]: c\nx [^c]\n").footnotes, 2);
    }

    #[test]
    fn footnote_lines_are_escaped_so_they_stay_visible() {
        assert_eq!(
            escape_footnote_lines("[^a]: b\n > [^c]: d\nx [^e]"),
            "\\[^a]: b\n > \\[^c]: d\nx [^e]"
        );
    }
}
