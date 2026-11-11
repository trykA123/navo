//! Maps a settings `Change` to the value written to `navo.toml`.

use navo_core::list_text::split_list;
use navo_store::SettingValue;

use super::{Change, Control, Value};

/// Returns the value stored for `change`, or `None` when the change is not stored.
pub fn stored_value(change: &Change) -> Option<SettingValue> {
    match (&change.value, change.field.spec().control) {
        (Value::Flag(flag), _) => Some(SettingValue::Bool(*flag)),
        (Value::Number(number), _) => Some(SettingValue::Integer(*number)),
        (Value::Text(text), _) if change.field.is_list() => {
            Some(SettingValue::List(split_list(text)))
        }
        (Value::Text(text), _) if text.is_empty() => None,
        (Value::Text(text), _) => Some(SettingValue::Text(text.to_string())),
        (Value::Pick(at), Control::Choices(options, _)) => options
            .get(*at)
            .map(|option| SettingValue::Text(option.toml.to_owned())),
        (Value::Pick(_) | Value::Unset | Value::Reset, _) => None,
    }
}
