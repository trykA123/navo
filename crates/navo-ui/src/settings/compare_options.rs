//! The `[compare]` options a compare runs with, read from the settings values (settings layer).

use navo_store::SettingValue;

use super::{Change, FIELDS, Section, Values, stored_value};
use crate::compare_diff::CompareOptions;

/// The `[compare]` settings the user sees in `values`; a table that does not parse
/// falls back to the defaults with a warning.
pub fn compare_options(values: &Values) -> CompareOptions {
    let mut table = serde_json::Map::new();
    for spec in FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Compare)
    {
        let Some(key) = spec.key.strip_prefix("compare.") else {
            continue;
        };
        let change = Change {
            field: spec.field,
            value: values.get(spec.field).clone(),
        };
        if let Some(value) = stored_value(&change) {
            table.insert(key.to_owned(), json_of(value));
        }
    }
    let compare = serde_json::from_value(serde_json::Value::Object(table))
        .inspect_err(|error| tracing::warn!(%error, "compare settings fall back to defaults"))
        .unwrap_or_default();
    CompareOptions {
        compare,
        fresh_hashes: false,
    }
}

/// One stored setting value as the JSON value serde reads the `[compare]` table from.
fn json_of(value: SettingValue) -> serde_json::Value {
    match value {
        SettingValue::Bool(flag) => flag.into(),
        SettingValue::Integer(number) => number.into(),
        SettingValue::Text(text) => text.into(),
        SettingValue::List(items) => items.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Field, Value};

    /// Default settings values give the default compare options.
    #[test]
    fn compare_options_of_default_values_equal_the_default_options() {
        assert_eq!(
            compare_options(&Values::default()),
            CompareOptions::default()
        );
    }

    /// Non-default compare values reach the options through the stored-settings rules.
    #[test]
    fn non_default_values_reach_the_compare_options() {
        let changes = [
            (Field::CompareBudgetMib, Value::Number(100)),
            (
                Field::CompareIgnoreList,
                Value::Text("*.{o,obj}, target".into()),
            ),
            (Field::CompareRespectGitignore, Value::Flag(false)),
        ];
        let values = changes
            .into_iter()
            .fold(Values::default(), |values, (field, value)| {
                values.with(&Change { field, value })
            });
        let options = compare_options(&values);
        assert_eq!(options.compare.memory_budget_mib, 256);
        assert_eq!(options.compare.ignore_list, ["*.{o,obj}", "target"]);
        assert!(!options.compare.respect_gitignore);
    }
}
