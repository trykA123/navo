//! The stored `[compare]` table as the options a compare runs with.

use navo_store::{Compare, SettingValue};

use crate::settings::{Change, FIELDS, Section, Values, stored_value};

/// The compare settings a diff or folder compare runs with (NC53).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompareOptions {
    /// The stored `[compare]` table.
    pub compare: Compare,
}

impl CompareOptions {
    /// The `[compare]` settings the user sees in `values`; a table that does not parse
    /// falls back to the defaults with a warning.
    pub fn from_values(values: &Values) -> Self {
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
        Self { compare }
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
