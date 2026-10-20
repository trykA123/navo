use super::*;
use crate::app_menu::Setting;
use crate::modifier_hints::HintsMode;
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
        let home = match spec.section {
            Section::Display => spec.group().map(str::to_lowercase),
            section => Some(section.label().to_lowercase()),
        };
        assert_eq!(Some(spec.table().to_owned()), home, "{}", spec.key);
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
    let last = first_row(Section::Display);
    assert_eq!(next_section(last, true), 0);
    assert_eq!(section_of(next_section(0, false)), Section::Display);
    assert_eq!(next_section(FIELDS.len() - 1, true), 0);
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

#[test]
fn appearance_has_the_modifier_hint_rows() {
    let hints = spec(Field::ModifierHints);
    assert_eq!(hints.section, Section::Appearance);
    assert_eq!(hints.key, "appearance.modifier-hints");
    let Control::Choices(options, Look::Segments) = hints.control else {
        panic!("modifier hints are not segments: {:?}", hints.control);
    };
    let toml: Vec<&str> = options.iter().map(|option| option.toml).collect();
    assert_eq!(toml, ["off", "fkeys", "all"]);
    let delay = spec(Field::ModifierHintsDelay);
    assert_eq!(delay.section, Section::Appearance);
    assert_eq!(delay.key, "appearance.modifier-hints-delay-ms");
    assert_eq!(delay.control, Control::Number);
    assert!(Field::ModifierHints.is_applied());
    assert!(Field::ModifierHintsDelay.is_applied());
}

#[test]
fn modifier_hints_default_to_every_shortcut_after_400_ms() {
    let values = Values::default();
    assert_eq!(values.modifier_hints(), HintsMode::All);
    assert_eq!(values.modifier_hints_delay_ms(), 400);
    let change = Change::modifier_hints(HintsMode::Fkeys);
    assert_eq!(change.key(), "appearance.modifier-hints");
    assert_eq!(values.with(&change).modifier_hints(), HintsMode::Fkeys);
}

#[test]
fn the_display_section_groups_the_window_and_each_viewer_in_order() {
    let keys: Vec<(Option<&str>, &str)> = FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Display)
        .map(|spec| (spec.group(), spec.key))
        .collect();
    let groups: Vec<Option<&str>> = keys.iter().map(|(group, _)| *group).collect();
    let expected = [
        ("Window", 12),
        ("Text", 2),
        ("Image", 1),
        ("Markdown", 1),
        ("Preview", 2),
        ("Automotive", 5),
    ];
    let expected: Vec<Option<&str>> = expected
        .iter()
        .flat_map(|(group, count)| std::iter::repeat_n(Some(*group), *count))
        .collect();
    assert_eq!(groups, expected);
    let keys: Vec<&str> = keys.into_iter().map(|(_, key)| key).collect();
    assert_eq!(keys[0], "window.view.tabs-with-one");
    assert_eq!(keys[11], "window.view.contents");
    assert_eq!(
        keys[12..],
        [
            "text.view.line-numbers",
            "text.view.line-ends",
            "image.view.details",
            "markdown.view.preview-split",
            "preview.view.type-bars",
            "preview.view.largest-files",
            "automotive.view.preset",
            "automotive.view.inspector",
            "automotive.view.memory-strip",
            "automotive.view.charts",
            "automotive.view.row-bars",
        ]
    );
    assert!(!Field::AutomotiveCharts.is_applied());
    assert!(Field::AutomotiveInspector.is_applied());
}

#[test]
fn the_inspector_opens_by_itself_only_in_rich_or_when_custom_asks() {
    let custom = |inspector: bool| {
        Values::default()
            .with(&Change {
                field: Field::AutomotivePreset,
                value: Value::Pick(2),
            })
            .with(&Change {
                field: Field::AutomotiveInspector,
                value: Value::Flag(inspector),
            })
    };
    let rich = Values::default().with(&Change {
        field: Field::AutomotivePreset,
        value: Value::Pick(1),
    });
    let simple_with_flag = Values::default().with(&Change {
        field: Field::AutomotiveInspector,
        value: Value::Flag(true),
    });
    assert!(!Values::default().opens_inspector());
    assert!(!simple_with_flag.opens_inspector());
    assert!(rich.opens_inspector());
    assert!(custom(true).opens_inspector());
    assert!(!custom(false).opens_inspector());
}

#[test]
fn every_display_row_has_a_one_sentence_hint() {
    for spec in FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Display)
    {
        assert!(spec.help.ends_with('.'), "{}", spec.key);
        assert_eq!(spec.help.matches(". ").count(), 0, "{}", spec.key);
    }
}

#[test]
fn text_parts_default_to_line_numbers_on_and_line_ends_off() {
    let values = Values::default();
    assert!(values.flag(Field::TextLineNumbers));
    assert!(!values.flag(Field::TextLineEnds));
    assert!(!values.flag(Field::ImageDetails));
    assert!(values.is_shown(None, Field::TextLineNumbers));
}

#[test]
fn the_markdown_split_and_both_preview_parts_default_to_on() {
    let values = Values::default();
    for field in [
        Field::MarkdownPreviewSplit,
        Field::PreviewTypeBars,
        Field::PreviewLargestFiles,
    ] {
        assert!(values.flag(field), "{}", field.spec().key);
        assert!(!values.is_changed(field));
        assert!(field.is_applied());
    }
    let hidden = values.with(&Change {
        field: Field::PreviewTypeBars,
        value: Value::Flag(false),
    });
    assert!(hidden.is_changed(Field::PreviewTypeBars));
    assert!(hidden.flag(Field::PreviewLargestFiles));
}

#[test]
fn every_window_part_is_on_by_default_and_reads_back() {
    let values = Values::default();
    assert_eq!(WindowParts::of(&values), WindowParts::default());
    assert_eq!(WindowParts::default(), WindowParts::all(true));
    let hidden = values.with(&Change {
        field: Field::WindowSizeColumn,
        value: Value::Flag(false),
    });
    assert!(!WindowParts::of(&hidden).size_column);
    assert!(WindowParts::of(&hidden).date_column);
    assert!(hidden.is_changed(Field::WindowSizeColumn));
    assert!(Field::WindowContents.is_applied());
}

#[test]
fn only_the_twelve_window_fields_are_window_parts() {
    let parts: Vec<Field> = FIELDS
        .iter()
        .map(|spec| spec.field)
        .filter(|field| field.is_window_part())
        .collect();
    assert_eq!(parts.len(), 12);
    assert!(
        FIELDS
            .iter()
            .filter(|spec| spec.key.starts_with("window."))
            .all(|spec| spec.field.is_window_part())
    );
}

#[test]
fn the_scroll_child_of_a_row_counts_the_group_headings_above_it() {
    let child =
        |field: Field| child_of(FIELDS.iter().position(|spec| spec.field == field).unwrap());
    assert_eq!(child(Field::Theme), Some(1));
    assert_eq!(child(Field::WindowTabsWithOne), Some(2));
    assert_eq!(child(Field::WindowContents), Some(13));
    assert_eq!(child(Field::TextLineNumbers), Some(15));
    assert_eq!(child(Field::ImageDetails), Some(18));
}

#[test]
fn the_copy_mode_row_sits_in_its_own_section_with_plain_help() {
    let row = spec(Field::CopyMode);
    assert_eq!(row.section, Section::Copy);
    assert_eq!(row.key, "copy.mode");
    assert_eq!(
        row.help,
        "Auto copies several files at once where the drives allow; Standard copies one file at a time."
    );
    assert!(matches!(row.control, Control::Choices(choices, Look::Segments) if choices.len() == 2));
    assert!(Field::CopyMode.is_applied());
}

#[test]
fn the_copy_mode_starts_auto_and_follows_the_pick() {
    let values = Values::default();
    assert_eq!(values.copy_mode(), rusty_fs::CopyMode::Auto);
    assert!(!values.is_changed(Field::CopyMode));
    let standard = values.with(&Change {
        field: Field::CopyMode,
        value: Value::Pick(1),
    });
    assert_eq!(standard.copy_mode(), rusty_fs::CopyMode::Standard);
    assert!(standard.is_changed(Field::CopyMode));
}
