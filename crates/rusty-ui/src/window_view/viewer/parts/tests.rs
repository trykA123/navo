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
fn keep_returns_only_the_parts_that_differ_and_forgets_the_toggles() {
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
    assert_eq!(parts, ViewParts::default());
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
    chosen.choose(&AUTOMOTIVE, 1);
    assert!(chosen.is_shown(&AUTOMOTIVE, Field::AutomotiveRowBars, &settings));
    chosen.choose(&AUTOMOTIVE, 2);
    assert!(chosen.is_shown(&AUTOMOTIVE, Field::AutomotiveInspector, &settings));
    assert!(!chosen.is_shown(&AUTOMOTIVE, Field::AutomotiveCharts, &settings));
}

#[test]
fn choosing_a_preset_is_ignored_where_the_viewer_has_none() {
    let mut parts = ViewParts::default();
    parts.choose(&TEXT, 1);
    assert_eq!(parts, ViewParts::default());
}
