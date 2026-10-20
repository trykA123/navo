use super::find::lines;
use super::*;

fn matching(text: &str) -> Vec<Field> {
    let filter = Filter::new(text);
    FIELDS
        .iter()
        .filter(|spec| filter.admits(spec))
        .map(|spec| spec.field)
        .collect()
}

fn shift(key: &str, old: &str, new: &str) -> DefaultShift {
    DefaultShift {
        key: key.to_owned(),
        old: old.to_owned(),
        new: new.to_owned(),
    }
}

#[test]
fn find_matches_the_label_ignoring_case() {
    assert_eq!(matching("INTERFACE font"), [Field::Font]);
    assert_eq!(matching("Folders FIRST"), [Field::FoldersFirst]);
}

#[test]
fn find_matches_the_key() {
    assert_eq!(matching("files.sort-desc"), [Field::SortDescending]);
}

#[test]
fn find_matches_the_help_text() {
    assert_eq!(matching("largest, newest"), [Field::SortDescending]);
    assert_eq!(matching("ctrl+f6"), [Field::WindowSizeColumn]);
}

#[test]
fn find_needs_every_word_and_an_empty_find_keeps_all_rows() {
    assert_eq!(matching("sort descending"), [Field::SortDescending]);
    assert_eq!(matching("   ").len(), FIELDS.len());
    assert!(matching("zzzz").is_empty());
}

#[test]
fn headings_without_a_match_do_not_appear() {
    let filter = Filter::new("files.sort-desc");
    let shown = lines(&filter, Section::Appearance);
    assert_eq!(shown[0], Line::Section(Section::Files));
    assert!(shown.iter().all(|line| !matches!(line, Line::Group(_))));
    assert_eq!(
        shown
            .iter()
            .filter(|line| matches!(line, Line::Section(_)))
            .count(),
        1
    );
}

#[test]
fn a_group_heading_shows_only_with_a_match_under_it() {
    let filter = Filter::new("line numbers");
    assert_eq!(
        lines(&filter, Section::Appearance),
        [
            Line::Section(Section::Display),
            Line::Group("Text"),
            Line::Row(
                FIELDS
                    .iter()
                    .position(|s| s.field == Field::TextLineNumbers)
                    .unwrap()
            ),
        ]
    );
}

#[test]
fn rows_move_through_matches_only() {
    let filter = Filter::new("files.sort");
    let first = filter.first_row().unwrap();
    let next = moved(&filter, first, true);
    assert_eq!(FIELDS[first].field, Field::Sort);
    assert_eq!(FIELDS[next].field, Field::SortDescending);
    assert_eq!(moved(&filter, next, true), next);
    assert_eq!(moved(&filter, next, false), first);
}

#[test]
fn tab_skips_sections_without_a_match() {
    let filter = Filter::new("line numbers");
    let row = filter.first_row().unwrap();
    assert_eq!(next_section(&filter, row, true), row);
    let both = Filter::new("e");
    assert_ne!(next_section(&both, 0, true), 0);
}

#[test]
fn a_row_outside_the_find_moves_to_the_first_match() {
    let filter = Filter::new("files.sort");
    assert_eq!(FIELDS[filter.nearest(0)].field, Field::Sort);
    let sort = filter.first_row().unwrap();
    assert_eq!(filter.nearest(sort), sort);
}

#[test]
fn the_scroll_child_follows_the_filtered_list() {
    let filter = Filter::new("files.sort");
    let descending = FIELDS
        .iter()
        .position(|s| s.field == Field::SortDescending)
        .unwrap();
    assert_eq!(child_of(&filter, descending), Some(2));
}

#[test]
fn a_default_shift_reads_as_key_old_to_new_with_the_field_label() {
    let moved = Moved::of(&shift("files.sort", "\"name\"", "\"date\"")).unwrap();
    assert_eq!(moved.field, Field::Sort);
    assert_eq!(moved.line(), "files.sort: name → date");
    assert_eq!(
        moved.keep_change(),
        Some(Change {
            field: Field::Sort,
            value: Value::Pick(0)
        })
    );
}

#[test]
fn a_default_shift_keeps_flags_numbers_and_text() {
    let flag = Moved::of(&shift("files.folders-first", "false", "true")).unwrap();
    assert_eq!(flag.keep_change().unwrap().value, Value::Flag(false));
    let number = Moved::of(&shift("appearance.font-size", "12", "13")).unwrap();
    assert_eq!(number.keep_change().unwrap().value, Value::Number(12));
    let text = Moved::of(&shift("appearance.font", "\"Segoe UI\"", "\"Inter\"")).unwrap();
    assert_eq!(
        text.keep_change().unwrap().value,
        Value::Text("Segoe UI".into())
    );
}

#[test]
fn a_shift_for_an_unknown_key_has_no_row_and_an_unknown_old_value_no_keep() {
    assert!(Moved::of(&shift("nothing.here", "1", "2")).is_none());
    let moved = Moved::of(&shift("files.sort", "\"oldest\"", "\"date\"")).unwrap();
    assert_eq!(moved.keep_change(), None);
}

fn on_shortcuts(filter: &str) -> SettingsBody {
    SettingsBody::new(Values::default(), 0)
        .shortcuts(Some(div().into_any_element()))
        .filter(Filter::new(filter))
}

#[test]
fn tab_from_shortcuts_lands_on_a_shown_row_or_nowhere() {
    let density = FIELDS
        .iter()
        .position(|s| s.field == Field::Density)
        .unwrap();
    assert_eq!(
        on_shortcuts("appearance.density").section_event(true),
        Some(SheetEvent::Select(density))
    );
    assert_eq!(on_shortcuts("files.sort").section_event(true), None);
    assert_eq!(on_shortcuts("zzz").section_event(false), None);
    assert!(on_shortcuts("").section_event(true).is_some());
}

#[test]
fn a_hidden_row_gets_no_key_events() {
    let row = FIELDS
        .iter()
        .position(|s| s.field == Field::FontSize)
        .unwrap();
    let values = Values::default().with(&Change {
        field: Field::FontSize,
        value: Value::Number(14),
    });
    let body = SettingsBody::new(values, row).filter(Filter::new("zzz"));
    assert!(body.reset_event().is_none());
    assert_eq!(body.event_for(Nudge::Forward), None);
    assert_eq!(body.flip_event(), None);
    assert_eq!(body.edit_event(), None);
}
