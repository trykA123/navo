use super::{AUTOMOTIVE, AUTOMOTIVE_LAYOUT, AUTOMOTIVE_TABLE, IMAGE, LayoutParts, TEXT, ViewParts};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::viewer::body::MenuEntry;
use crate::window_view::viewer::view_menu::view_entries;

fn set(values: &Values, field: Field, value: Value) -> Values {
    values.with(&Change { field, value })
}

fn rich() -> Values {
    set(&Values::default(), Field::AutomotivePreset, Value::Pick(1))
}

#[test]
fn without_a_toggle_the_parts_follow_the_settings() {
    let parts = ViewParts::default();
    let settings = set(&Values::default(), Field::TextLineEnds, Value::Flag(true));
    assert!(parts.is_shown(&TEXT, Field::TextLineNumbers, &settings));
    assert!(parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
    assert!(!parts.is_shown(&IMAGE, Field::ImageDetails, &settings));
}

#[test]
fn a_toggle_flips_one_part_and_leaves_the_settings_alone() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    assert!(parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
    assert!(parts.is_shown(&TEXT, Field::TextLineNumbers, &settings));
    assert!(!settings.flag(Field::TextLineEnds));
}

#[test]
fn a_settings_change_reaches_every_part_the_viewer_did_not_toggle() {
    let mut parts = ViewParts::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &Values::default());
    let changed = set(
        &Values::default(),
        Field::TextLineNumbers,
        Value::Flag(false),
    );
    let changed = set(&changed, Field::TextLineEnds, Value::Flag(false));
    assert!(!parts.is_shown(&TEXT, Field::TextLineNumbers, &changed));
    assert!(parts.is_shown(&TEXT, Field::TextLineEnds, &changed));
}

#[test]
fn keep_returns_only_the_parts_that_differ() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    parts.toggle(&TEXT, Field::TextLineNumbers, &settings);
    parts.toggle(&TEXT, Field::TextLineNumbers, &settings);
    let kept = parts.keep(&settings);
    assert_eq!(
        kept,
        [Change {
            field: Field::TextLineEnds,
            value: Value::Flag(true),
        }]
    );
}

#[test]
fn reset_brings_back_the_settings() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&IMAGE, Field::ImageDetails, &settings);
    assert!(parts.is_shown(&IMAGE, Field::ImageDetails, &settings));
    parts.reset();
    assert!(!parts.is_shown(&IMAGE, Field::ImageDetails, &settings));
}

#[test]
fn a_part_toggled_under_a_preset_turns_the_preset_custom_and_keeps_the_others_as_shown() {
    let mut parts = ViewParts::default();
    let settings = rich();
    parts.toggle(&AUTOMOTIVE, Field::AutomotiveCharts, &settings);
    let preset = parts
        .effective(&settings)
        .get(Field::AutomotivePreset)
        .clone();
    assert_eq!(preset, Value::Pick(2));
    assert!(!parts.is_shown(&AUTOMOTIVE, Field::AutomotiveCharts, &settings));
    for field in [
        Field::AutomotiveInspector,
        Field::AutomotiveMemoryStrip,
        Field::AutomotiveRowBars,
    ] {
        assert!(parts.is_shown(&AUTOMOTIVE, field, &settings), "{field:?}");
    }
}

#[test]
fn a_simple_preset_hides_every_part_whatever_its_flag() {
    let parts = ViewParts::default();
    let settings = set(
        &Values::default(),
        Field::AutomotiveInspector,
        Value::Flag(true),
    );
    assert!(!parts.is_shown(&AUTOMOTIVE, Field::AutomotiveInspector, &settings));
    let mut chosen = parts;
    chosen.choose(&AUTOMOTIVE, 1, &settings);
    assert!(chosen.is_shown(&AUTOMOTIVE, Field::AutomotiveRowBars, &settings));
}

#[test]
fn choosing_a_preset_is_ignored_where_the_viewer_has_none() {
    let mut parts = ViewParts::default();
    parts.choose(&TEXT, 1, &Values::default());
    assert_eq!(parts, ViewParts::default());
}

fn automotive_shown(parts: &ViewParts, settings: &Values) -> [bool; 4] {
    [
        Field::AutomotiveInspector,
        Field::AutomotiveMemoryStrip,
        Field::AutomotiveCharts,
        Field::AutomotiveRowBars,
    ]
    .map(|field| parts.is_shown(&AUTOMOTIVE, field, settings))
}

fn custom_with(flags: [bool; 4]) -> Values {
    let fields = [
        Field::AutomotiveInspector,
        Field::AutomotiveMemoryStrip,
        Field::AutomotiveCharts,
        Field::AutomotiveRowBars,
    ];
    fields.iter().zip(flags).fold(
        set(&Values::default(), Field::AutomotivePreset, Value::Pick(2)),
        |values, (field, on)| set(&values, *field, Value::Flag(on)),
    )
}

#[test]
fn a_toggle_under_custom_overrides_only_that_part() {
    let mut parts = ViewParts::default();
    let settings = custom_with([true, false, true, false]);
    parts.toggle(&AUTOMOTIVE, Field::AutomotiveCharts, &settings);
    let later = custom_with([true, true, true, true]);
    assert_eq!(automotive_shown(&parts, &later), [true, true, false, true]);
}

#[test]
fn rich_then_custom_keeps_every_part_shown() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.choose(&AUTOMOTIVE, 1, &settings);
    parts.choose(&AUTOMOTIVE, 2, &settings);
    assert_eq!(automotive_shown(&parts, &settings), [true; 4]);
}

#[test]
fn simple_and_rich_set_every_part_to_their_own_value() {
    let mut parts = ViewParts::default();
    let settings = custom_with([true, false, true, false]);
    parts.choose(&AUTOMOTIVE, 0, &settings);
    parts.choose(&AUTOMOTIVE, 2, &settings);
    assert_eq!(automotive_shown(&parts, &settings), [false; 4]);
    parts.choose(&AUTOMOTIVE, 1, &settings);
    parts.choose(&AUTOMOTIVE, 2, &settings);
    assert_eq!(automotive_shown(&parts, &settings), [true; 4]);
}

#[test]
fn custom_chosen_under_custom_keeps_the_parts_shown() {
    let mut parts = ViewParts::default();
    let settings = custom_with([true, false, true, false]);
    parts.choose(&AUTOMOTIVE, 2, &settings);
    assert_eq!(
        automotive_shown(&parts, &settings),
        [true, false, true, false]
    );
}

#[test]
fn a_kept_part_stays_an_override_until_a_clean_save_is_reported() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    let kept = parts.keep(&settings);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        parts.settle(&[change(Field::TextLineEnds, true)], false),
        Some(false)
    );
    assert!(parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
    parts.keep(&settings);
    assert_eq!(
        parts.settle(&[change(Field::TextLineEnds, true)], true),
        Some(true)
    );
    assert!(!parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
    assert_eq!(parts, ViewParts::default());
}

#[test]
fn a_save_of_other_fields_does_not_settle_a_keep() {
    let mut parts = ViewParts::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &Values::default());
    parts.keep(&Values::default());
    assert_eq!(parts.settle(&[change(Field::Theme, true)], true), None);
}

fn memory_parts_shown(parts: &ViewParts, settings: &Values) -> [bool; 3] {
    [
        Field::AutomotiveMemoryStrip,
        Field::AutomotiveCharts,
        Field::AutomotiveRowBars,
    ]
    .map(|field| parts.is_shown(&AUTOMOTIVE, field, settings))
}

#[test]
fn a_toggle_in_the_table_view_keeps_the_memory_parts_of_a_rich_preset_shown() {
    let mut parts = ViewParts::default();
    let settings = rich();
    parts.toggle(&AUTOMOTIVE_TABLE, Field::AutomotiveInspector, &settings);
    assert!(!parts.is_shown(&AUTOMOTIVE_TABLE, Field::AutomotiveInspector, &settings));
    assert_eq!(memory_parts_shown(&parts, &settings), [true; 3]);
    let kept = parts.keep(&settings);
    let saved = kept.iter().fold(settings.clone(), |all, one| all.with(one));
    assert_eq!(memory_parts_shown(&ViewParts::default(), &saved), [true; 3]);
    assert!(!saved.is_shown(AUTOMOTIVE.preset, Field::AutomotiveInspector));
}

#[test]
fn custom_chosen_in_the_table_view_keeps_the_memory_parts_of_a_rich_preset_shown() {
    let mut parts = ViewParts::default();
    let settings = rich();
    parts.choose(&AUTOMOTIVE_TABLE, 2, &settings);
    assert_eq!(memory_parts_shown(&parts, &settings), [true; 3]);
    assert!(parts.is_shown(&AUTOMOTIVE_TABLE, Field::AutomotiveInspector, &settings));
    let kept = parts.keep(&settings);
    let saved = kept.iter().fold(settings.clone(), |all, one| all.with(one));
    assert_eq!(memory_parts_shown(&ViewParts::default(), &saved), [true; 3]);
}

fn change(field: Field, on: bool) -> Change {
    Change {
        field,
        value: Value::Flag(on),
    }
}

#[test]
fn a_second_keep_while_the_first_save_runs_leaves_the_keep_pending() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    assert_eq!(parts.keep(&settings).len(), 1);
    let applied = set(&settings, Field::TextLineEnds, Value::Flag(true));
    assert!(parts.keep(&applied).is_empty());
    assert!(parts.is_pending());
}

#[test]
fn a_failed_first_save_still_reports_after_a_second_keep() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    parts.keep(&settings);
    let applied = set(&settings, Field::TextLineEnds, Value::Flag(true));
    parts.keep(&applied);
    let sent = [change(Field::TextLineEnds, true)];
    assert_eq!(parts.settle(&sent, false), Some(false));
    assert!(!parts.is_pending());
    assert!(parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
}

#[test]
fn an_older_save_of_the_same_field_does_not_settle_a_newer_keep() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    parts.keep(&settings);
    parts.toggle(&TEXT, Field::TextLineEnds, &settings);
    let applied = set(&settings, Field::TextLineEnds, Value::Flag(true));
    let newer = parts.keep(&applied);
    assert_eq!(newer, [change(Field::TextLineEnds, false)]);
    assert_eq!(
        parts.settle(&[change(Field::TextLineEnds, true)], true),
        None
    );
    assert!(parts.is_pending());
    assert_eq!(parts.settle(&newer, true), Some(true));
}

#[test]
fn rich_turns_every_layout_part_on() {
    let layout = LayoutParts::of(&rich());
    assert_eq!(
        layout,
        LayoutParts {
            strip: true,
            bars: true,
            who: true,
            row_bars: true,
        }
    );
    assert_eq!(LayoutParts::of(&Values::default()), LayoutParts::default());
}

#[test]
fn a_layout_toggle_makes_the_preset_custom_and_keeps_the_other_parts() {
    let mut parts = ViewParts::default();
    let settings = rich();
    parts.toggle(&AUTOMOTIVE_LAYOUT, Field::AutomotiveWhoUses, &settings);
    let shown = parts.effective(&settings);
    assert_eq!(shown.get(Field::AutomotivePreset), &Value::Pick(2));
    assert_eq!(
        LayoutParts::of(&shown),
        LayoutParts {
            strip: true,
            bars: true,
            who: false,
            row_bars: true,
        }
    );
}

#[test]
fn keep_as_default_writes_the_two_new_keys() {
    let mut parts = ViewParts::default();
    let settings = Values::default();
    parts.choose(&AUTOMOTIVE_LAYOUT, 1, &settings);
    let kept = parts.keep(&settings);
    for field in [Field::AutomotiveUsageBars, Field::AutomotiveWhoUses] {
        assert!(kept.iter().any(|change| change.field == field), "{field:?}");
    }
}

#[test]
fn the_view_menu_lists_five_parts_for_elf_and_map_and_four_for_hex() {
    let parts = |set| {
        let entries = view_entries(set, &Values::default(), false, false);
        let breaks: Vec<usize> = (0..entries.len())
            .filter(|at| matches!(entries[*at], MenuEntry::Break))
            .collect();
        breaks[1] - breaks[0] - 1
    };
    assert_eq!(parts(&AUTOMOTIVE_LAYOUT), 5);
    assert_eq!(parts(&AUTOMOTIVE), 4);
}
