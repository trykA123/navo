use super::*;

fn header_then(items: Vec<MenuItem>) -> Vec<Row> {
    rows(&ShellMenu { items }, "")
}

#[test]
fn first_skips_the_header() {
    let rows = rows(&zip_menu(), "");
    assert_eq!(Selection::first(&rows).index(), Some(1));
}

#[test]
fn next_skips_disabled_and_separators() {
    let rows = rows(&zip_menu(), "");
    let rename = Selection::first(&rows).next(&rows).next(&rows);
    assert_eq!(rename.index(), Some(3));
    assert_eq!(rename.next(&rows).index(), Some(5));
}

#[test]
fn next_wraps_from_the_last_row() {
    let rows = rows(&zip_menu(), "");
    let last = Selection::first(&rows).previous(&rows);
    assert_eq!(last.index(), Some(rows.len() - 1));
    assert_eq!(last.next(&rows).index(), Some(1));
}

#[test]
fn previous_skips_the_windows_header() {
    let rows = rows(&zip_menu(), "");
    let open = rows
        .iter()
        .position(|row| matches!(row, Row::Item(item) if item.text == "Open"))
        .unwrap();
    let mut selection = Selection::first(&rows);
    while selection.index() != Some(open) {
        selection = selection.next(&rows);
    }
    assert_eq!(selection.previous(&rows).index(), Some(6));
}

#[test]
fn enter_runs_a_command() {
    let rows = rows(&zip_menu(), "");
    assert_eq!(
        Selection::first(&rows).enter(&rows),
        Some(Choice::Run(ItemId::Ferrite(FerriteAction::CopyPath)))
    );
}

#[test]
fn enter_and_right_open_a_submenu() {
    let rows = rows(&zip_menu(), "7-zip");
    let selection = Selection::first(&rows);
    assert_eq!(selection.enter(&rows), Some(Choice::OpenSubmenu(vec![8])));
    assert_eq!(selection.open_submenu(&rows), Some(vec![8]));
}

#[test]
fn right_on_a_command_does_nothing() {
    let rows = rows(&zip_menu(), "");
    assert_eq!(Selection::first(&rows).open_submenu(&rows), None);
}

#[test]
fn enter_on_more_shows_rare_items() {
    let rows = rows(&zip_menu(), "");
    let more = Selection::first(&rows).previous(&rows);
    assert_eq!(more.enter(&rows), Some(Choice::ShowMore));
}

#[test]
fn all_disabled_rows_are_still_reachable() {
    let rows = header_then(vec![
        MenuItem {
            enabled: false,
            ..shell(1, "A", Group::Open)
        },
        MenuItem {
            enabled: false,
            ..shell(2, "B", Group::Open)
        },
    ]);
    let first = Selection::first(&rows);
    assert_eq!(first.index(), Some(1));
    assert_eq!(first.next(&rows).index(), Some(2));
    assert_eq!(first.enter(&rows), None);
}

#[test]
fn empty_rows_select_nothing() {
    let selection = Selection::first(&[]);
    assert_eq!(selection.index(), None);
    assert_eq!(selection.next(&[]).index(), None);
    assert_eq!(selection.enter(&[]), None);
}

#[test]
fn stale_index_restarts_from_the_top() {
    let long = rows(&zip_menu(), "");
    let short = rows(&zip_menu(), "copy");
    let stale = Selection::first(&long).previous(&long);
    assert_eq!(stale.next(&short).index(), Some(0));
}
