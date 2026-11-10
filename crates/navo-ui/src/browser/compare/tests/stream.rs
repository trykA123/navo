//! What the Stream reads from a folder compare (V20): rows under the folder depth first, the pair
//! of one row, a stamp that moves with every row update, and `compare_reveal` on the way back.

use super::super::super::tests::settle;
use super::super::{CompareFilter, RowStatus};
use super::mode::{deliver, root_rows, selected, start};
use super::{browser, cursor_names, pane_locations, row, rows, sides};

/// The `sides` tree plus `docs/deep/c.txt` on both sides.
fn deep_sides() -> std::sync::Arc<navo_fs::FakeFs> {
    let fs = sides();
    for side in ["left", "right"] {
        fs.add_folder(&format!("{side}/docs/deep"));
        fs.add_file(&format!("{side}/docs/deep/c.txt"), 5);
    }
    fs
}

/// `root_rows` plus the folder `docs/deep` and its file `docs/deep/c.txt`.
fn deep_rows() -> super::super::CompareUpdate {
    rows(
        1,
        vec![
            row(6, "docs/deep", RowStatus::Different),
            row(7, "docs/deep/c.txt", RowStatus::Different),
        ],
    )
}

#[test]
fn rows_under_the_folder_come_depth_first_and_in_engine_order() {
    let fs = deep_sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    deliver(&mut browser, &calls, deep_rows());
    let mode = browser.compare_mode().expect("compare mode");
    let paths: Vec<String> = mode
        .rows_under_folder()
        .into_iter()
        .map(|r| r.path)
        .collect();
    let expected = [
        "docs",
        "docs/report.txt",
        "docs/deep",
        "docs/deep/c.txt",
        "a.txt",
        "b.txt",
        "old",
    ];
    assert_eq!(paths, expected);
    let pair = mode.pair_of(7).expect("the pair of c.txt");
    assert_eq!(pair.left, Some(fs.location("left/docs/deep/c.txt")));
    assert_eq!(pair.right, Some(fs.location("right/docs/deep/c.txt")));
    assert!(mode.pair_of(99).is_none(), "a row that never arrived");
}

#[test]
fn the_rows_stamp_moves_with_every_row_update() {
    let fs = deep_sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    let stamp = |b: &crate::browser::Browser| b.compare_mode().expect("mode").rows_stamp();
    let first = stamp(&browser);
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(5, "docs/report.txt", RowStatus::Same)]),
    );
    assert_ne!(stamp(&browser), first, "a changed row moves the stamp");
    let second = stamp(&browser);
    deliver(&mut browser, &calls, rows(1, Vec::new()));
    assert_eq!(stamp(&browser), second, "an empty batch does not");
}

#[test]
fn reveal_shows_a_row_two_folders_down_in_both_panes() {
    let fs = deep_sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    deliver(&mut browser, &calls, deep_rows());
    assert!(browser.compare_reveal("docs/deep/c.txt"));
    settle(&mut browser);
    let mode = browser.compare_mode().expect("still comparing");
    assert_eq!(mode.folder(), "docs/deep");
    assert_eq!(selected(&browser).as_deref(), Some("docs/deep/c.txt"));
    let deep = [
        fs.location("left/docs/deep").as_str().to_owned(),
        fs.location("right/docs/deep").as_str().to_owned(),
    ];
    assert_eq!(pane_locations(&browser), deep);
    assert_eq!(
        cursor_names(&browser),
        [Some("c.txt".into()), Some("c.txt".into())]
    );
}

#[test]
fn reveal_in_the_shown_folder_selects_and_clears_a_filter_that_hides_the_row() {
    let fs = deep_sides();
    let (mut browser, calls) = browser(&fs);
    let roots = pane_locations(&browser);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    let mode = browser.compare_mode_mut().expect("compare mode");
    mode.set_filter(CompareFilter::Orphans);
    assert!(browser.compare_reveal("a.txt"));
    settle(&mut browser);
    let mode = browser.compare_mode().expect("compare mode");
    assert_eq!(mode.filter(), CompareFilter::All);
    assert_eq!(selected(&browser).as_deref(), Some("a.txt"));
    assert_eq!(pane_locations(&browser), roots, "the panes stay");
    browser.leave_compare();
    assert!(!browser.compare_reveal("a.txt"), "outside a compare");
}
