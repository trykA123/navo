use super::{AUTOMOTIVE, IMAGE, TEXT, ViewParts};
use crate::settings::{Change, Field, Value, Values};

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
    let mut chosen = parts.clone();
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
    assert_eq!(parts.settle(&[Field::TextLineEnds], false), Some(false));
    assert!(parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
    parts.keep(&settings);
    assert_eq!(parts.settle(&[Field::TextLineEnds], true), Some(true));
    assert!(!parts.is_shown(&TEXT, Field::TextLineEnds, &settings));
    assert_eq!(parts, ViewParts::default());
}

#[test]
fn a_save_of_other_fields_does_not_settle_a_keep() {
    let mut parts = ViewParts::default();
    parts.toggle(&TEXT, Field::TextLineEnds, &Values::default());
    parts.keep(&Values::default());
    assert_eq!(parts.settle(&[Field::Theme], true), None);
}
