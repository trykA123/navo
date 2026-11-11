//! Converts a loaded pair into the UI's `PairText` and `Card` (V1), and row spans.

use std::ops::Range;

use navo_compare::text::intraline::SideLine;
use navo_compare::text::{
    Alignment, CardPlan, CardRow as PlanRow, Fallback, Hunk, LinePair, Row, SpanSide, Whole,
    intraline,
};
use navo_ui::compare_diff::{
    Card, CardRow, DiffRow, HunkView, PairFacts, PairText, RowSpans, ViewedKey,
};

use super::copy::shown_digest;
use super::load::Loaded;

/// The pair text for a text or one-sided pair; `None` for a summary-only kind.
#[must_use]
pub(crate) fn to_pair_text(loaded: &Loaded) -> Option<PairText> {
    let alignment = loaded.alignment.as_ref()?;
    if !matches!(
        loaded.kind,
        navo_ui::compare_diff::PairKind::Text | navo_ui::compare_diff::PairKind::OneSided { .. }
    ) {
        return None;
    }
    let left = lines_of(loaded.left.as_ref());
    let right = lines_of(loaded.right.as_ref());
    let hunks = hunk_views(alignment, &hunk_rows(alignment));
    Some(PairText {
        left,
        right,
        left_starts: loaded
            .left
            .as_ref()
            .map(|whole| whole.starts.clone())
            .unwrap_or_default(),
        right_starts: loaded
            .right
            .as_ref()
            .map(|whole| whole.starts.clone())
            .unwrap_or_default(),
        rows: alignment.rows.iter().map(|row| diff_row(*row)).collect(),
        hunks,
        facts: facts_of(loaded, alignment),
        viewed: ViewedKey(loaded.viewed),
    })
}

/// Build a Stream card from the plan; line text is copied only for shown rows.
#[must_use]
pub(crate) fn to_card(loaded: &Loaded, plan: &CardPlan) -> Card {
    let mut rows = Vec::with_capacity(plan.rows.len() + 1);
    let mut hunks = Vec::with_capacity(plan.hunks.len());
    let mut added = 0u32;
    let mut removed = 0u32;
    if let Some(alignment) = loaded.alignment.as_ref() {
        for row in &plan.rows {
            rows.push(card_row(loaded, alignment, *row));
        }
        for (index, range) in plan.hunks.iter().enumerate() {
            if let Some(hunk) = alignment.hunks.get(index) {
                hunks.push(hunk_view(alignment, hunk, range.clone()));
                added = added.saturating_add(count(hunk.right_lines.len()));
                removed = removed.saturating_add(count(hunk.left_lines.len()));
            }
        }
        added = added.saturating_add(count(
            alignment
                .hunks
                .iter()
                .skip(plan.shown_hunks)
                .map(|hunk| hunk.right_lines.len())
                .sum(),
        ));
        removed = removed.saturating_add(count(
            alignment
                .hunks
                .iter()
                .skip(plan.shown_hunks)
                .map(|hunk| hunk.left_lines.len())
                .sum(),
        ));
    }
    if plan.more_hunks > 0 {
        rows.push(CardRow::More {
            hunks: count(plan.more_hunks),
        });
    }
    Card {
        kind: loaded.kind,
        rows,
        hunks,
        added,
        removed,
        viewed_key: ViewedKey(loaded.viewed),
        viewed: false,
        viewable: loaded.viewable,
    }
}

/// Changed spans for the requested alignment rows; fallback rows give no spans.
#[must_use]
pub(crate) fn spans(loaded: &Loaded, rows: Range<u32>) -> Vec<RowSpans> {
    let (Some(alignment), Some(left), Some(right)) = (
        loaded.alignment.as_ref(),
        loaded.left.as_ref(),
        loaded.right.as_ref(),
    ) else {
        return Vec::new();
    };
    let end = usize::try_from(rows.end).unwrap_or(usize::MAX);
    let start = usize::try_from(rows.start).unwrap_or(0);
    let end = end.min(alignment.rows.len());
    let mut found = Vec::new();
    for (index, row) in alignment.rows.iter().enumerate().take(end).skip(start) {
        if let Some(spans) = changed_spans(index, *row, left, right) {
            found.push(spans);
        }
    }
    found
}

/// Build one Changed row's spans, or `None` when it has no pair or intraline falls back.
fn changed_spans(index: usize, row: Row, left: &Whole, right: &Whole) -> Option<RowSpans> {
    let Row::Changed {
        left: left_index,
        right: right_index,
    } = row
    else {
        return None;
    };
    let left_line = left.lines.get(left_index)?;
    let right_line = right.lines.get(right_index)?;
    let left_side = SideLine {
        line: left_line,
        start: left.starts.get(left_index).copied().unwrap_or(0),
        layout: left.layout,
        lossless: left.lossless,
    };
    let right_side = SideLine {
        line: right_line,
        start: right.starts.get(right_index).copied().unwrap_or(0),
        layout: right.layout,
        lossless: right.lossless,
    };
    let found = intraline(LinePair {
        left: left_side,
        right: right_side,
    });
    if found.fallback.is_some() {
        return None;
    }
    let (left_ranges, right_ranges) = byte_spans(&found, &left_line.text, &right_line.text);
    if left_ranges.is_empty() && right_ranges.is_empty() {
        return None;
    }
    Some(RowSpans {
        row: count(index),
        left: left_ranges,
        right: right_ranges,
    })
}

/// Map the engine's rows to the UI's `DiffRow`.
fn diff_row(row: Row) -> DiffRow {
    match row {
        Row::Equal { left, right } => DiffRow::Equal(count(left), count(right)),
        Row::Changed { left, right } => DiffRow::Changed(count(left), count(right)),
        Row::Deleted { left } => DiffRow::Deleted(count(left)),
        Row::Inserted { right } => DiffRow::Inserted(count(right)),
    }
}

/// Build the UI facts: encodings, endings, equality flags and engine fallbacks.
fn facts_of(loaded: &Loaded, alignment: &Alignment) -> PairFacts {
    PairFacts {
        left_encoding: encoding_id(loaded.left.as_ref()),
        right_encoding: encoding_id(loaded.right.as_ref()),
        left_endings: endings_id(loaded.left.as_ref()),
        right_endings: endings_id(loaded.right.as_ref()),
        raw_equal: alignment.raw_equal,
        display_equal: alignment.display_equal,
        fallbacks: match alignment.fallback {
            Some(Fallback::TokenCap) => vec!["compare-fallback-token-cap"],
            None => Vec::new(),
        },
    }
}

/// The Fluent id of a side's encoding, or `none` when the side is absent.
fn encoding_id(whole: Option<&Whole>) -> &'static str {
    let Some(whole) = whole else {
        return "compare-encoding-none";
    };
    match whole.layout.text.encoding.name() {
        "UTF-8" => "compare-encoding-utf-8",
        "UTF-16LE" => "compare-encoding-utf-16le",
        "UTF-16BE" => "compare-encoding-utf-16be",
        "windows-1250" => "compare-encoding-windows-1250",
        "windows-1251" => "compare-encoding-windows-1251",
        "windows-1252" | "ISO-8859-1" => "compare-encoding-windows-1252",
        "windows-1253" => "compare-encoding-windows-1253",
        "windows-1254" => "compare-encoding-windows-1254",
        "windows-1255" => "compare-encoding-windows-1255",
        "windows-1256" => "compare-encoding-windows-1256",
        "windows-1257" => "compare-encoding-windows-1257",
        "windows-1258" => "compare-encoding-windows-1258",
        "ISO-8859-2" => "compare-encoding-iso-8859-2",
        "ISO-8859-15" => "compare-encoding-iso-8859-15",
        "KOI8-R" => "compare-encoding-koi8-r",
        "Shift_JIS" => "compare-encoding-shift-jis",
        "EUC-JP" => "compare-encoding-euc-jp",
        "GBK" => "compare-encoding-gbk",
        "Big5" => "compare-encoding-big5",
        "EUC-KR" => "compare-encoding-euc-kr",
        "IBM866" => "compare-encoding-ibm866",
        _ => "compare-encoding-other",
    }
}

/// The Fluent id of a side's line endings, or `none` when the side is absent.
fn endings_id(whole: Option<&Whole>) -> &'static str {
    let Some(whole) = whole else {
        return "compare-endings-none";
    };
    let (mut lf, mut crlf) = (false, false);
    for line in &whole.lines {
        match line.ending {
            navo_compare::text::LineEnding::Lf => lf = true,
            navo_compare::text::LineEnding::CrLf => crlf = true,
            navo_compare::text::LineEnding::None => {}
        }
    }
    match (lf, crlf) {
        (true, true) => "compare-endings-mixed",
        (true, false) => "compare-endings-lf",
        (false, true) => "compare-endings-crlf",
        (false, false) => "compare-endings-none",
    }
}

/// The text of a side's lines, or an empty vector when the side is absent.
fn lines_of(whole: Option<&Whole>) -> Vec<String> {
    whole
        .map(|whole| whole.lines.iter().map(|line| line.text.clone()).collect())
        .unwrap_or_default()
}

/// Map each hunk to its own rows in `Alignment.rows`.
pub(super) fn hunk_rows(alignment: &Alignment) -> Vec<Range<usize>> {
    let mut ranges = vec![0..0; alignment.hunks.len()];
    let mut hunk_at = 0usize;
    for (at, row) in alignment.rows.iter().enumerate() {
        if matches!(row, Row::Equal { .. }) {
            continue;
        }
        while hunk_at < alignment.hunks.len() && !row_in_hunk(&alignment.hunks[hunk_at], row) {
            hunk_at += 1;
        }
        let Some(range) = ranges.get_mut(hunk_at) else {
            break;
        };
        if range.start == range.end {
            range.start = at;
        }
        range.end = at + 1;
    }
    ranges
}

/// Say whether an aligned row's line indexes fall inside the hunk's ranges.
fn row_in_hunk(hunk: &Hunk, row: &Row) -> bool {
    match *row {
        Row::Equal { .. } => false,
        Row::Changed { left, right } => {
            hunk.left_lines.contains(&left) && hunk.right_lines.contains(&right)
        }
        Row::Deleted { left } => hunk.left_lines.contains(&left),
        Row::Inserted { right } => hunk.right_lines.contains(&right),
    }
}

/// Build one UI hunk from an engine hunk and its card-row or alignment-row range.
fn hunk_views(alignment: &Alignment, rows: &[Range<usize>]) -> Vec<HunkView> {
    alignment
        .hunks
        .iter()
        .zip(rows)
        .map(|(hunk, range)| hunk_view(alignment, hunk, range.clone()))
        .collect()
}

/// Build one UI hunk; the 16-byte digest is the left side's when the hunk is copyable.
fn hunk_view(alignment: &Alignment, hunk: &Hunk, rows: Range<usize>) -> HunkView {
    HunkView {
        rows: count(rows.start)..count(rows.end),
        left_bytes: hunk.left_bytes.clone(),
        right_bytes: hunk.right_bytes.clone(),
        digest: alignment
            .digests
            .get(hunk)
            .map(|digest| shown_digest(&digest)),
        added: count(hunk.right_lines.len()),
        removed: count(hunk.left_lines.len()),
    }
}

/// Build one UI card row from the plan row.
fn card_row(loaded: &Loaded, alignment: &Alignment, row: PlanRow) -> CardRow {
    match row {
        PlanRow::Gap { lines } => CardRow::Gap {
            lines: count(lines),
        },
        PlanRow::Row { index, hunk } => {
            let (left, right) = alignment.rows.get(index).map_or((None, None), |row| {
                row_text(*row, loaded.left.as_ref(), loaded.right.as_ref())
            });
            CardRow::Line {
                row: alignment
                    .rows
                    .get(index)
                    .map_or(DiffRow::Equal(0, 0), |row| diff_row(*row)),
                hunk: count(hunk),
                left,
                right,
            }
        }
    }
}

/// The text of an aligned row's left and right lines, when each side has one.
fn row_text(
    row: Row,
    left: Option<&Whole>,
    right: Option<&Whole>,
) -> (Option<String>, Option<String>) {
    match row {
        Row::Equal { left: l, right: r } | Row::Changed { left: l, right: r } => {
            (line_text(left, l), line_text(right, r))
        }
        Row::Deleted { left: l } => (line_text(left, l), None),
        Row::Inserted { right: r } => (None, line_text(right, r)),
    }
}

/// Clone one side's line text at a line index.
fn line_text(whole: Option<&Whole>, index: usize) -> Option<String> {
    whole?.lines.get(index).map(|line| line.text.clone())
}

/// Convert the intraline character spans to UTF-8 byte ranges into each row `String`.
fn byte_spans(
    found: &navo_compare::text::Intraline,
    left_text: &str,
    right_text: &str,
) -> (Vec<Range<u32>>, Vec<Range<u32>>) {
    let spans = if found.chars.is_empty() {
        &found.words
    } else {
        &found.chars
    };
    let mut left = Vec::new();
    let mut right = Vec::new();
    for span in spans {
        let text = match span.side {
            SpanSide::Left => left_text,
            SpanSide::Right => right_text,
        };
        let target = match span.side {
            SpanSide::Left => &mut left,
            SpanSide::Right => &mut right,
        };
        target.push(char_byte(text, span.chars.start)..char_byte(text, span.chars.end));
    }
    (left, right)
}

/// The UTF-8 byte offset of a character index in a line, clamped to the line length.
fn char_byte(text: &str, index: usize) -> u32 {
    let offset = text
        .char_indices()
        .nth(index)
        .map_or(text.len(), |(offset, _)| offset);
    count(offset)
}

/// Saturating conversion from a size to the UI's `u32`.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
