use super::files;
use crate::Selection;

#[test]
fn million_rows_mark_all_counts_every_row() {
    let listing = files(1_000_000);
    let mut selection = Selection::new(1_000_000);
    selection.mark_all(&listing, false);
    assert_eq!(selection.count(), 1_000_000);
}
