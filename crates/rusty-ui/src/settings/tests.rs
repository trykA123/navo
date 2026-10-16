use super::*;
use crate::app_menu::Setting;
use crate::tokens::{Density, ThemeChoice};

fn spec(field: Field) -> &'static FieldSpec {
    field.spec()
}

#[test]
fn every_field_sits_at_its_own_index() {
    for (index, spec) in FIELDS.iter().enumerate() {
        assert_eq!(spec.field.index(), index, "{:?}", spec.field);
    }
}

#[test]
fn every_key_is_unique_and_lives_in_its_section() {
    for spec in &FIELDS {
        let table = spec.key.split('.').next().unwrap_or_default();
        assert_eq!(table, spec.section.label().to_lowercase(), "{}", spec.key);
        assert_eq!(
            FIELDS.iter().filter(|other| other.key == spec.key).count(),
            1
        );
    }
}

#[test]
fn sections_run_in_order_without_gaps() {
    let order: Vec<Section> = FIELDS.iter().map(|spec| spec.section).collect();
    let mut seen: Vec<Section> = Vec::new();
    for section in order {
        if seen.last() != Some(&section) {
            assert!(!seen.contains(&section), "{section:?} split");
            seen.push(section);
        }
    }
    assert_eq!(seen, Section::ALL);
}

#[test]
fn the_right_arrow_steps_a_choice_and_stops_at_the_end() {
    let sort = spec(Field::Sort);
    assert_eq!(
        nudged(sort, &Value::Pick(0), Nudge::Forward),
        Some(Value::Pick(1))
    );
    assert_eq!(nudged(sort, &Value::Pick(3), Nudge::Forward), None);
    assert_eq!(nudged(sort, &Value::Pick(0), Nudge::Back), None);
}

#[test]
fn space_cycles_a_choice() {
    let theme = spec(Field::Theme);
    assert_eq!(
        nudged(theme, &Value::Pick(2), Nudge::Flip),
        Some(Value::Pick(0))
    );
}

#[test]
fn arrows_set_a_toggle_and_space_flips_it() {
    let toggle = spec(Field::ShowSystem);
    assert_eq!(
        nudged(toggle, &Value::Flag(false), Nudge::Forward),
        Some(Value::Flag(true))
    );
    assert_eq!(nudged(toggle, &Value::Flag(true), Nudge::Forward), None);
    assert_eq!(
        nudged(toggle, &Value::Flag(true), Nudge::Back),
        Some(Value::Flag(false))
    );
    assert_eq!(
        nudged(toggle, &Value::Flag(true), Nudge::Flip),
        Some(Value::Flag(false))
    );
}

#[test]
fn arrows_step_a_number_and_text_needs_editing() {
    let size = spec(Field::FontSize);
    assert_eq!(
        nudged(size, &Value::Number(13), Nudge::Forward),
        Some(Value::Number(14))
    );
    assert_eq!(nudged(size, &Value::Number(1), Nudge::Back), None);
    let font = spec(Field::Font);
    assert_eq!(
        nudged(font, &Value::Text("Inter".into()), Nudge::Forward),
        None
    );
}

#[test]
fn tab_jumps_to_the_next_section_and_wraps() {
    assert_eq!(section_of(next_section(0, true)), Section::Files);
    let last = first_row(Section::Editor);
    assert_eq!(next_section(last, true), 0);
    assert_eq!(section_of(next_section(0, false)), Section::Editor);
}

#[test]
fn rows_move_across_sections_and_stop_at_the_ends() {
    let last_appearance = first_row(Section::Files) - 1;
    assert_eq!(section_of(moved(last_appearance, true)), Section::Files);
    assert_eq!(moved(0, false), 0);
    assert_eq!(moved(FIELDS.len() - 1, true), FIELDS.len() - 1);
}

#[test]
fn defaults_show_hidden_files_and_follow_the_system() {
    let values = Values::default();
    assert_eq!(values.theme(), ThemeChoice::System);
    assert_eq!(values.density(), Density::Normal);
    assert!(values.show_hidden());
    assert_eq!(values.editor_path(), None);
}

#[test]
fn a_menu_tile_toggle_becomes_a_change_of_its_field() {
    let values = Values::default();
    let change = values.toggled(Setting::HiddenFiles);
    assert_eq!(change, Change::show_hidden(false));
    assert_eq!(change.key(), "files.hidden-files");
    assert!(!values.with(&change).show_hidden());
    let theme = values.toggled(Setting::Theme);
    assert_eq!(values.with(&theme).theme(), ThemeChoice::Light);
}
