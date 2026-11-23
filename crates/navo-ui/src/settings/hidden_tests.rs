//! Tests for retired Settings rows: hidden from the page and search, their values kept (AU9).

use super::*;

const HIDDEN: [Field; 7] = [
    Field::LightTheme,
    Field::DarkTheme,
    Field::AutomotiveMemoryStrip,
    Field::AutomotiveCharts,
    Field::AutomotiveRowBars,
    Field::AutomotiveUsageBars,
    Field::AutomotiveWhoUses,
];

#[test]
fn a_hidden_field_is_not_in_the_pages_row_list_or_in_search() {
    for field in HIDDEN {
        assert!(!field.is_shown(), "{field:?}");
        let spec = field.spec();
        assert!(!Filter::default().admits(spec), "{field:?} on the page");
        assert!(!Filter::new(spec.key).admits(spec), "{field:?} in search");
        let rows = lines(&Filter::new(spec.key), spec.section);
        assert!(!rows.contains(&Line::Row(field.index())), "{field:?} row");
    }
    assert!(Field::TerminalProvider.is_shown());
}

#[test]
fn a_hidden_fields_saved_value_survives_load_and_save() {
    for field in HIDDEN {
        let saved = match field.spec().control {
            Control::Text => Value::Text("kept".into()),
            Control::Toggle => Value::Flag(true),
            _ => unreachable!("{field:?}"),
        };
        let loaded = Values::from_fn(|spec| {
            if spec.field == field {
                saved.clone()
            } else {
                Values::default().get(spec.field).clone()
            }
        });
        assert_eq!(loaded.get(field), &saved, "{field:?}");
        let change = loaded.change_of(field);
        assert!(stored_value(&change).is_some(), "{field:?} is saved");
        assert_eq!(Values::default().with(&change).get(field), &saved);
    }
}
