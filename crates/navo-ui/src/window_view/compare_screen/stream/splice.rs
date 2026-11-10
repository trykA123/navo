//! Row edits for the Stream's `ListState`: which runs of rows changed between two builds, so
//! one `splice` per run keeps the top visible row where it is (D25).

use std::ops::Range;

use crate::compare_diff::StreamRow;

/// One edit: the `old` rows of the last build become `new` rows of the next build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Edit {
    pub(super) old: Range<usize>,
    pub(super) new: usize,
}

/// The edits that turn `old` into `new`, last first, so each applies to the list in turn.
/// When both builds list the same cards in the same order, each changed card is its own
/// edit: a card arriving above the view then shifts the view by its rows and nothing else.
/// Otherwise one edit spans the rows between the common prefix and the common suffix.
pub(super) fn edits(old: &[StreamRow], new: &[StreamRow]) -> Vec<Edit> {
    let (old_cards, new_cards) = (cards(old), cards(new));
    let same_cards = old_cards.len() == new_cards.len()
        && old_cards
            .iter()
            .zip(&new_cards)
            .all(|(a, b)| old[a.start] == new[b.start]);
    if !same_cards {
        return trim(old, new, 0).into_iter().collect();
    }
    let mut out: Vec<Edit> = old_cards
        .iter()
        .zip(&new_cards)
        .filter_map(|(a, b)| trim(&old[a.clone()], &new[b.clone()], a.start))
        .collect();
    out.reverse();
    out
}

/// The row ranges of each card: a header and the rows after it, or the rules group alone.
/// Rows before the first header (none in practice) form a card of their own.
fn cards(rows: &[StreamRow]) -> Vec<Range<usize>> {
    let mut starts: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(at, row)| *at == 0 || matches!(row, StreamRow::Header(_) | StreamRow::RulesGroup))
        .map(|(at, _)| at)
        .collect();
    starts.push(rows.len());
    starts.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

/// The single edit between `old` and `new` after their common prefix and suffix, shifted by
/// `offset`; `None` when they are equal.
fn trim(old: &[StreamRow], new: &[StreamRow], offset: usize) -> Option<Edit> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old.len() - prefix - suffix;
    let added = new.len() - prefix - suffix;
    (removed > 0 || added > 0).then(|| Edit {
        old: offset + prefix..offset + prefix + removed,
        new: added,
    })
}

#[cfg(test)]
mod tests {
    use super::{Edit, edits};
    use crate::compare_diff::{Half, StreamRow};

    /// A line row of `item`.
    fn line(item: usize, card_row: u32) -> StreamRow {
        StreamRow::Line {
            item,
            card_row,
            half: Half::Both,
        }
    }

    /// The row count after `edits`, as the list counts it.
    fn count_after(old: usize, edits: &[Edit]) -> usize {
        edits
            .iter()
            .fold(old, |len, edit| len - edit.old.len() + edit.new)
    }

    #[test]
    fn compare_stream_a_card_arriving_is_one_edit_of_its_placeholder() {
        let old = vec![
            StreamRow::Header(0),
            StreamRow::Placeholder(0),
            StreamRow::Header(1),
            line(1, 0),
        ];
        let new = vec![
            StreamRow::Header(0),
            line(0, 0),
            line(0, 1),
            line(0, 2),
            StreamRow::Header(1),
            line(1, 0),
        ];
        assert_eq!(edits(&old, &new), vec![Edit { old: 1..2, new: 3 }]);
    }

    #[test]
    fn compare_stream_two_cards_give_two_edits_last_first() {
        let old = vec![
            StreamRow::Header(0),
            StreamRow::Placeholder(0),
            StreamRow::Header(1),
            line(1, 0),
            StreamRow::Header(2),
            StreamRow::Placeholder(2),
        ];
        let new = vec![
            StreamRow::Header(0),
            line(0, 0),
            line(0, 1),
            StreamRow::Header(1),
            line(1, 0),
            StreamRow::Header(2),
            StreamRow::Summary(2),
        ];
        let found = edits(&old, &new);
        assert_eq!(
            found,
            vec![Edit { old: 5..6, new: 1 }, Edit { old: 1..2, new: 2 }]
        );
        assert_eq!(count_after(old.len(), &found), new.len());
    }

    #[test]
    fn compare_stream_a_folded_card_drops_its_rows() {
        let old = vec![
            StreamRow::Header(0),
            line(0, 0),
            line(0, 1),
            StreamRow::Header(1),
        ];
        let new = vec![StreamRow::Header(0), StreamRow::Header(1)];
        assert_eq!(edits(&old, &new), vec![Edit { old: 1..3, new: 0 }]);
    }

    #[test]
    fn compare_stream_equal_builds_need_no_edit() {
        let rows = vec![StreamRow::Header(0), line(0, 0), StreamRow::RulesGroup];
        assert!(edits(&rows, &rows).is_empty());
    }

    #[test]
    fn compare_stream_other_cards_give_one_edit_between_prefix_and_suffix() {
        let old = vec![
            StreamRow::Header(0),
            StreamRow::Header(1),
            StreamRow::Header(2),
        ];
        let new = vec![StreamRow::Header(0), StreamRow::Header(2)];
        assert_eq!(edits(&old, &new), vec![Edit { old: 1..2, new: 0 }]);
        assert_eq!(edits(&[], &old), vec![Edit { old: 0..0, new: 3 }]);
    }
}
