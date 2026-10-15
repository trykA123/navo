use std::ops::Range;

use rusty_core::paging::HEX_WIDTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Paint {
    Hit,
    Mark(u8),
}

pub(super) struct Painted<'a> {
    pub hit: Option<&'a Range<u64>>,
    pub marks: &'a [(Range<u64>, u8)],
}

struct RowPaint<'a> {
    hit: Option<&'a Range<u64>>,
    marks: Vec<&'a (Range<u64>, u8)>,
}

impl<'a> Painted<'a> {
    fn within(&self, span: &Range<u64>) -> RowPaint<'a> {
        let overlaps = |range: &Range<u64>| range.start < span.end && span.start < range.end;
        RowPaint {
            hit: self.hit.filter(|hit| overlaps(hit)),
            marks: self
                .marks
                .iter()
                .filter(|(range, _)| overlaps(range))
                .collect(),
        }
    }
}

impl RowPaint<'_> {
    fn at(&self, offset: u64) -> Option<Paint> {
        if self.hit.is_some_and(|hit| hit.contains(&offset)) {
            return Some(Paint::Hit);
        }
        self.marks
            .iter()
            .find(|(range, _)| range.contains(&offset))
            .map(|(_, colour)| Paint::Mark(*colour))
    }
}

pub(super) struct TextCut {
    pub start: u64,
    pub shift: usize,
    pub limit: usize,
}

pub(super) fn shown_text(
    text: &str,
    cut: &TextCut,
    painted: &Painted<'_>,
) -> (String, Vec<(Range<usize>, Paint)>) {
    let row = painted.within(&(cut.start..cut.start + text.len() as u64));
    let mut shown = String::new();
    let mut runs: Vec<(Range<usize>, Paint)> = Vec::new();
    for (at, ch) in text.char_indices().skip(cut.shift).take(cut.limit) {
        let from = shown.len();
        shown.push(if ch.is_control() { ' ' } else { ch });
        let Some(paint) = row.at(cut.start + at as u64) else {
            continue;
        };
        match runs.last_mut() {
            Some((range, last)) if *last == paint && range.end == from => range.end = shown.len(),
            _ => runs.push((from..shown.len(), paint)),
        }
    }
    (shown, runs)
}

pub(super) fn hex_slots(start: u64, painted: &Painted<'_>) -> Vec<(usize, Paint)> {
    let row = painted.within(&(start..start + HEX_WIDTH));
    (0..HEX_WIDTH)
        .filter_map(|slot| Some((usize::try_from(slot).ok()?, row.at(start + slot)?)))
        .collect()
}

pub(super) fn line_end(start: u64, end: u64, next: u64) -> Option<&'static str> {
    match next.checked_sub(end.max(start)) {
        Some(2) => Some("CRLF"),
        Some(1) => Some("LF"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Paint, Painted, TextCut, hex_slots, line_end, shown_text};

    fn cut(start: u64) -> TextCut {
        TextCut {
            start,
            shift: 0,
            limit: 100,
        }
    }

    #[test]
    fn marks_and_the_hit_become_runs_over_the_shown_text() {
        let marks = [(102..104, 3), (108..110, 1)];
        let hit = 106..108;
        let painted = Painted {
            hit: Some(&hit),
            marks: &marks,
        };
        let (shown, runs) = shown_text("abcdefghijkl", &cut(100), &painted);
        assert_eq!(shown, "abcdefghijkl");
        assert_eq!(
            runs,
            [
                (2..4, Paint::Mark(3)),
                (6..8, Paint::Hit),
                (8..10, Paint::Mark(1))
            ]
        );
    }

    #[test]
    fn the_hit_wins_over_a_mark_on_the_same_bytes() {
        let marks = [(0..4, 0)];
        let hit = 2..4;
        let painted = Painted {
            hit: Some(&hit),
            marks: &marks,
        };
        let (_, runs) = shown_text("abcd", &cut(0), &painted);
        assert_eq!(runs, [(0..2, Paint::Mark(0)), (2..4, Paint::Hit)]);
    }

    #[test]
    fn control_characters_show_as_spaces_and_keep_their_place() {
        let marks = [(1..2, 5)];
        let painted = Painted {
            hit: None,
            marks: &marks,
        };
        let (shown, runs) = shown_text("a\tb", &cut(0), &painted);
        assert_eq!(shown, "a b");
        assert_eq!(runs, [(1..2, Paint::Mark(5))]);
    }

    #[test]
    fn hex_slots_report_each_painted_byte() {
        let marks = [(14..18, 2)];
        let painted = Painted {
            hit: None,
            marks: &marks,
        };
        assert_eq!(
            hex_slots(16, &painted),
            [(0, Paint::Mark(2)), (1, Paint::Mark(2))]
        );
    }

    #[test]
    fn line_ends_read_from_the_row_bounds() {
        assert_eq!(line_end(0, 5, 7), Some("CRLF"));
        assert_eq!(line_end(0, 5, 6), Some("LF"));
        assert_eq!(line_end(0, 5, 5), None);
    }
}
