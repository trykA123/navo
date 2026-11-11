//! Fake data for the compare diff gallery pages: one file pair with three changes, and a
//! Stream of six files. Everything is built here; no compare engine runs.

use std::sync::Arc;

use navo_ui::browser::{CompareKind, CompareRow, CompareSide, RowStatus};
use navo_ui::compare_diff::{
    Card, CardRow, DiffRow, HunkView, PairFacts, PairKind, PairText, RowSpans, SpanCache,
    StreamModel, StreamUpdate, ViewedKey, stream_items,
};

use crate::compare_page::{card_line, sample_card};

/// One line of the fake pair.
enum Op {
    Same(&'static str),
    Change(&'static str, &'static str),
    Delete(&'static str),
    Insert(&'static str),
}

const OPS: [Op; 18] = [
    Op::Same("use std::path::Path;"),
    Op::Same(""),
    Op::Same("/// Counts the entries under `root`."),
    Op::Change(
        "fn walk(root: &Path) -> u32 {",
        "fn walk(root: &Path) -> u64 {",
    ),
    Op::Change("    let mut seen = 0;", "    let mut seen = 0u64;"),
    Op::Delete("    // old counter"),
    Op::Insert("    // counts every entry, folders too"),
    Op::Same("    for entry in read_dir(root) {"),
    Op::Same("        seen += 1;"),
    Op::Same("    }"),
    Op::Same("    seen"),
    Op::Same("}"),
    Op::Change("const LIMIT: u32 = 1_000;", "const LIMIT: u32 = 10_000;"),
    Op::Same(""),
    Op::Insert("fn files(root: &Path) -> u64 {"),
    Op::Insert("    walk(root) - folders(root)"),
    Op::Insert("}"),
    Op::Same("// end"),
];

/// A common-prefix and common-suffix byte range of `a` and `b` that differs, per side.
fn changed_words(a: &str, b: &str) -> (Vec<std::ops::Range<u32>>, Vec<std::ops::Range<u32>>) {
    let head = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let tail = a
        .bytes()
        .rev()
        .zip(b.bytes().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let tail = tail.min(a.len() - head).min(b.len() - head);
    let range = |s: &str| {
        let end = s.len() - tail;
        (head < end).then_some(head as u32..end as u32)
    };
    (
        range(a).into_iter().collect(),
        range(b).into_iter().collect(),
    )
}

/// The lines, rows, hunks and spans collected while the ops are read.
#[derive(Default)]
struct Builder {
    left: Vec<String>,
    right: Vec<String>,
    rows: Vec<DiffRow>,
    hunks: Vec<HunkView>,
    spans: Vec<RowSpans>,
}

impl Builder {
    /// Adds the lines and the row of `op`; changed words are recorded for a changed row.
    fn add(&mut self, op: &Op) {
        let at = self.rows.len() as u32;
        let (l, r) = (self.left.len() as u32, self.right.len() as u32);
        let row = match op {
            Op::Same(s) => {
                self.left.push((*s).to_owned());
                self.right.push((*s).to_owned());
                DiffRow::Equal(l, r)
            }
            Op::Change(a, b) => {
                let (left, right) = changed_words(a, b);
                self.spans.push(RowSpans {
                    row: at,
                    left,
                    right,
                });
                self.left.push((*a).to_owned());
                self.right.push((*b).to_owned());
                DiffRow::Changed(l, r)
            }
            Op::Delete(s) => {
                self.left.push((*s).to_owned());
                DiffRow::Deleted(l)
            }
            Op::Insert(s) => {
                self.right.push((*s).to_owned());
                DiffRow::Inserted(r)
            }
        };
        self.rows.push(row);
        let counts = match row {
            DiffRow::Equal(..) => return,
            DiffRow::Changed(..) => (1, 1),
            DiffRow::Deleted(_) => (0, 1),
            DiffRow::Inserted(_) => (1, 0),
        };
        self.count_in_hunk(at, counts);
    }

    /// Extends the hunk that ends at row `at`, or starts a new one.
    fn count_in_hunk(&mut self, at: u32, (added, removed): (u32, u32)) {
        match self.hunks.last_mut().filter(|h| h.rows.end == at) {
            Some(hunk) => {
                hunk.rows.end = at + 1;
                hunk.added += added;
                hunk.removed += removed;
            }
            None => self.hunks.push(HunkView {
                rows: at..at + 1,
                left_bytes: 0..0,
                right_bytes: 0..0,
                digest: Some([0; 16]),
                added,
                removed,
            }),
        }
    }
}

/// The fake pair, and the changed-word spans of its changed rows.
pub fn pair() -> (Arc<PairText>, SpanCache) {
    let mut build = Builder::default();
    OPS.iter().for_each(|op| build.add(op));
    let facts = PairFacts {
        left_encoding: "compare-encoding-utf-8",
        right_encoding: "compare-encoding-utf-8",
        left_endings: "compare-endings-lf",
        right_endings: "compare-endings-crlf",
        raw_equal: false,
        display_equal: false,
        fallbacks: Vec::new(),
    };
    let text = Arc::new(PairText {
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        left: build.left,
        right: build.right,
        rows: build.rows,
        hunks: build.hunks,
        facts,
        viewed: ViewedKey([0; 64]),
    });
    let mut cache = SpanCache::default();
    cache.request(0..text.rows.len() as u32);
    cache.insert(&text, build.spans);
    (text, cache)
}

/// The compare rows of the Stream, in path order.
fn stream_files() -> [CompareRow; 8] {
    let file = Some(CompareSide {
        kind: CompareKind::File,
        size: 1,
        modified_ms: None,
    });
    let row = |id: u64, path: &str, status, left: bool| CompareRow {
        id,
        path: path.to_owned(),
        left: file.filter(|_| left),
        right: file,
        status,
        contains_differences: false,
    };
    let (diff, rules) = (RowStatus::Different, RowStatus::SameAfterRules);
    [
        row(1, "assets/logo.bin", diff, true),
        row(2, "crates/core/src/legacy.rs", RowStatus::OnlyLeft, true),
        row(3, "crates/core/src/lib.rs", RowStatus::Checking, true),
        row(4, "crates/core/src/walk.rs", diff, true),
        row(5, "docs/guide.md", diff, true),
        row(6, "docs/notes.md", diff, true),
        row(7, "a.txt", rules, true),
        row(8, "b.txt", rules, true),
    ]
}

/// The rows of a text card with one change.
fn walk_rows() -> Vec<CardRow> {
    vec![
        card_line(
            DiffRow::Equal(10, 10),
            0,
            "fn walk(root: &Path) {",
            "fn walk(root: &Path) {",
        ),
        card_line(
            DiffRow::Changed(11, 11),
            0,
            "    let mut seen = 0;",
            "    let mut seen = 0u64;",
        ),
        card_line(DiffRow::Deleted(12), 0, "    // old counter", ""),
        card_line(DiffRow::Inserted(12), 0, "", "    // counts every entry"),
        card_line(DiffRow::Equal(13, 13), 0, "}", "}"),
    ]
}

/// The cards the app has built: summary, one-sided, changed, folded and viewed.
fn stream_cards() -> [(u64, Card); 5] {
    [
        (1, sample_card(PairKind::Binary, Vec::new(), false)),
        (
            2,
            sample_card(PairKind::OneSided { left: true }, Vec::new(), false),
        ),
        (4, sample_card(PairKind::Text, walk_rows(), false)),
        (5, sample_card(PairKind::Text, walk_rows(), false)),
        (6, sample_card(PairKind::Text, Vec::new(), true)),
    ]
}

/// A Stream of six files plus a rules group: changed (open), one-sided, summary, viewed,
/// checking and folded, with the outline drawer open.
pub fn stream() -> StreamModel {
    let mut model = StreamModel::default();
    model.set_items(1, stream_items(&stream_files(), ""));
    for (id, mut card) in stream_cards() {
        if card.kind != PairKind::Text {
            (card.added, card.removed) = (0, 0);
        }
        let card = Arc::new(card);
        model.apply(&StreamUpdate::Card {
            generation: 1,
            id,
            card,
        });
    }
    model.jump_to(4);
    model.toggle_fold();
    model.jump_to(3);
    model.set_outline(true);
    model
}
