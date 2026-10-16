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
    assert_eq!(selection.previous(&rows).index(), Some(7));
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
    assert_eq!(selection.enter(&rows), Some(Choice::OpenSubmenu(vec![9])));
    assert_eq!(selection.open_submenu(&rows), Some(vec![9]));
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

#[test]
fn keep_follows_the_command_when_a_lazy_submenu_fills() {
    let mut menu = ShellMenu {
        items: vec![
            lazy(1, "Send to", Group::Share),
            shell(2, "Delete", Group::Edit),
        ],
    };
    let old = rows(&menu, "de");
    let delete = Selection::first(&old);
    assert_eq!(delete.index(), Some(0));
    menu.set_submenu(
        &[0],
        Submenu::Filled(vec![shell(10, "Desktop", Group::Share)]),
    );
    let new = rows(&menu, "de");
    let kept = delete.keep(&old, &new);
    assert_eq!(kept.index(), Some(1));
    assert_eq!(
        kept.enter(&new),
        Some(Choice::Run(ItemId::Shell {
            command: 2,
            verb: None
        }))
    );
}

#[test]
fn keep_falls_back_to_first_when_the_command_is_gone() {
    let menu = zip_menu();
    let old = rows(&menu, "");
    let rename = Selection::first(&old).next(&old).next(&old);
    let new = rows(&menu, "copy");
    assert_eq!(rename.keep(&old, &new), Selection::first(&new));
}

#[test]
fn keep_stays_on_more() {
    let menu = zip_menu();
    let old = rows(&menu, "");
    let more = Selection::first(&old).previous(&old);
    let kept = more.keep(&old, &old);
    assert_eq!(kept.enter(&old), Some(Choice::ShowMore));
}

#[test]
fn pick_selects_an_item_and_refuses_a_header() {
    let rows = rows(&zip_menu(), "");
    assert_eq!(Selection::pick(&rows, 3).and_then(|s| s.index()), Some(3));
    assert_eq!(
        Selection::pick(&rows, 3).and_then(|s| s.path().map(<[usize]>::to_vec)),
        Some(vec![2])
    );
    assert_eq!(Selection::pick(&rows, 0), None);
}
