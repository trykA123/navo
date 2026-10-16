use pulldown_cmark::Options;

use super::{Refusal, TABLES_AS_TEXT};

const MAX_TABLE_CELLS: usize = 1_000_000;
const MAX_FOOTNOTE_LINES: usize = 2_000;

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    cells: usize,
    footnotes: usize,
}

fn is_container(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '>' | '-' | '+' | '*' | '.' | ')') || ch.is_ascii_digit()
}

fn is_delimiter_char(ch: char) -> bool {
    matches!(ch, '|' | ':' | '-' | ' ' | '\t')
}

fn is_blank(line: &str) -> bool {
    line.trim_end_matches([' ', '\t']).is_empty()
}

fn delimiter_columns(line: &str) -> Option<usize> {
    let start = line
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_delimiter_char(*ch))
        .last()
        .map_or(line.len(), |(at, _)| at);
    let (prefix, rest) = line.split_at(start);
    let is_delimiter = rest.contains('-') && prefix.chars().all(is_container);
    is_delimiter.then(|| rest.matches('|').count() + 1)
}

fn is_footnote_line(line: &str) -> bool {
    line.trim_start_matches(is_container).starts_with("[^")
}

#[derive(Default)]
struct Run {
    lines: usize,
    columns: Option<usize>,
}

impl Run {
    fn cells(&self) -> usize {
        self.columns
            .map_or(0, |columns| self.lines.saturating_mul(columns))
    }
}

fn count(source: &str) -> Counts {
    let mut counts = Counts::default();
    let mut run = Run::default();
    for line in source.lines() {
        if is_blank(line) {
            counts.cells = counts.cells.saturating_add(run.cells());
            run = Run::default();
            continue;
        }
        if is_footnote_line(line) {
            counts.footnotes += 1;
        }
        run.lines += 1;
        if let Some(columns) = delimiter_columns(line) {
            run.columns = Some(run.columns.map_or(columns, |widest| widest.max(columns)));
        }
    }
    counts.cells = counts.cells.saturating_add(run.cells());
    counts
}

pub(super) struct Plan {
    pub options: Options,
    pub notes: Vec<&'static str>,
}

pub(super) fn plan(source: &str) -> Result<Plan, Refusal> {
    let counts = count(source);
    if counts.footnotes > MAX_FOOTNOTE_LINES {
        return Err(Refusal::TooManyFootnotes);
    }
    let mut plan = Plan {
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
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::{Counts, count};

    #[test]
    fn a_run_counts_its_lines_times_its_widest_delimiter() {
        let table = "| a | b |\n|---|:-:|\n| 1 |\nx\n\nafter\n";
        assert_eq!(
            count(table),
            Counts {
                cells: 16,
                footnotes: 0
            }
        );
    }

    #[test]
    fn delimiters_are_found_behind_any_container_prefix() {
        assert_eq!(count("a\n> - 1. |-|-|\n").cells, 8);
        assert_eq!(count("a\n-:\n").cells, 2);
    }

    #[test]
    fn only_ascii_space_and_tab_make_a_blank_line() {
        assert_eq!(count("|-|\n \t\n|-|\n").cells, 6);
        assert_eq!(count("|-|\n\u{a0}\n|-|\n").cells, 9);
    }

    #[test]
    fn footnote_lines_count_behind_any_container_prefix() {
        assert_eq!(
            count("[^a]: b\n > [^b]: c\n- 1. [^c]\nx [^d]\n").footnotes,
            3
        );
    }
}
