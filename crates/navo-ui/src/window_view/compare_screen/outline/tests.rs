//! Tests for the outline's lines: folder headings, files in order, the rules count and search.

use super::{Line, lines};
use crate::browser::RowStatus;
use crate::compare_diff::{StreamItem, StreamList, StreamModel};

/// A model over files at `paths` (ids from 1) with `rules` same-after-rules files.
fn model(paths: &[&str], rules: usize) -> StreamModel {
    let items = (1..).zip(paths).map(|(id, path)| StreamItem {
        id,
        path: (*path).to_owned(),
        status: RowStatus::Different,
        settled: true,
    });
    let mut model = StreamModel::default();
    model.set_items(
        1,
        StreamList {
            items: items.collect(),
            rules,
        },
    );
    model
}

#[test]
fn a_heading_starts_each_folder_and_the_rules_count_ends_the_list() {
    let m = model(&["a.txt", "src/b.rs", "src/c.rs", "z/d.rs"], 2);
    assert_eq!(
        lines(&m),
        [
            Line::Folder(String::new()),
            Line::File(0),
            Line::Folder("src".into()),
            Line::File(1),
            Line::File(2),
            Line::Folder("z".into()),
            Line::File(3),
            Line::Rules(2),
        ]
    );
}

#[test]
fn a_search_keeps_only_matching_files_and_drops_the_rules_count() {
    let mut m = model(&["a.txt", "src/b.rs", "src/c.rs"], 3);
    m.begin_search(1);
    m.apply(&crate::compare_diff::StreamUpdate::Hits {
        generation: 1,
        query: 1,
        hits: vec![(3, vec![0, 1])],
        searched: 3,
        total: 3,
        done: true,
    });
    assert_eq!(lines(&m), [Line::Folder("src".into()), Line::File(2)]);
    assert_eq!(m.hit_count(3), Some(2));
    assert_eq!(m.hit_count(1), None);
}
