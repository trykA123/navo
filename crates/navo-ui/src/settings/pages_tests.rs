//! Tests for the Settings pages: every setting sits on exactly one page, and the arrows
//! walk the rows in page order.

use super::view::section_label;
use super::*;

const PAGE_NAMES: [&str; 10] = [
    "Appearance",
    "Window",
    "Files",
    "Copy and delete",
    "Search",
    "Viewers",
    "Tools",
    "Tray and clipboard",
    "Keyboard",
    "About",
];

/// Each shown field with the page the README map puts it on (a sample from every page).
const MOVED: [(Field, &str); 11] = [
    (Field::ModifierHints, "Keyboard"),
    (Field::WindowExtWidth, "Window"),
    (Field::TextSyntaxTheme, "Viewers"),
    (Field::RightButton, "Files"),
    (Field::SpaceMeasuresFolders, "Files"),
    (Field::TerminalProvider, "Tools"),
    (Field::EditorPath, "Tools"),
    (Field::CompareBudgetMib, "Tools"),
    (Field::RetentionDays, "Copy and delete"),
    (Field::ClipboardOn, "Tray and clipboard"),
    (Field::CheckUpdates, "About"),
];

#[test]
fn the_nav_lists_the_ten_pages_in_order() {
    assert_eq!(
        Section::ALL.map(section_label).as_slice(),
        PAGE_NAMES.map(SharedString::from).as_slice()
    );
}

#[test]
fn every_shown_field_lands_on_exactly_one_page() {
    for spec in FIELDS.iter().filter(|spec| spec.field.is_shown()) {
        let pages: Vec<Section> = Section::ALL
            .into_iter()
            .filter(|page| rows_in(*page).any(|(_, row)| row.field == spec.field))
            .collect();
        assert_eq!(
            pages,
            [spec.section],
            "{:?} must be on one page",
            spec.field
        );
    }
}

#[test]
fn retired_fields_are_on_no_page() {
    for spec in FIELDS.iter().filter(|spec| !spec.field.is_shown()) {
        let on_page = Section::ALL
            .into_iter()
            .any(|page| rows_in(page).any(|(_, row)| row.field == spec.field));
        assert!(!on_page, "{:?} is retired and stays hidden", spec.field);
    }
}

#[test]
fn fields_land_where_the_map_says() {
    for (field, page) in MOVED {
        assert_eq!(section_label(field.spec().section), page, "{field:?}");
    }
}

#[test]
fn down_from_the_last_row_of_a_page_goes_to_the_next_page_first_row() {
    let filter = Filter::default();
    assert_eq!(
        moved(&filter, Field::Font.index(), true),
        Field::WindowTabsWithOne.index(),
    );
    assert_eq!(
        moved(&filter, Field::WindowTabsWithOne.index(), false),
        Field::Font.index(),
    );
}
