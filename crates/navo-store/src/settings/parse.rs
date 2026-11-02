use super::{Settings, SettingsError};

// Keys older builds wrote; they still load and are ignored, and saves leave them in the file.
const LEGACY_KEYS: [&str; 1] = ["files.show-hidden"];

pub fn parse(text: &str) -> Result<(Settings, Vec<String>), SettingsError> {
    let mut unknown = Vec::new();
    let mut settings: Settings = toml::de::Deserializer::parse(text)
        .and_then(|de| serde_ignored::deserialize(de, |path| unknown.push(path.to_string())))
        .map_err(|e| invalid(text, &e))?;
    unknown.retain(|key| !LEGACY_KEYS.contains(&key.as_str()));
    let order = key_positions(text);
    unknown.sort_by_key(|key| order.iter().position(|k| k == key).unwrap_or(usize::MAX));
    unknown.extend(settings.layout.drop_invalid(text));
    Ok((settings, unknown))
}

pub fn parse_retention(
    text: &str,
) -> Result<(super::RetentionSettings, Vec<String>), SettingsError> {
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Policy {
        retention: super::RetentionSettings,
    }
    let mut unknown = Vec::new();
    let policy: Policy = toml::de::Deserializer::parse(text)
        .and_then(|de| serde_ignored::deserialize(de, |path| unknown.push(path.to_string())))
        .map_err(|error| invalid(text, &error))?;
    unknown.retain(|key| key.starts_with("retention."));
    Ok((policy.retention, unknown))
}

#[must_use]
pub fn to_toml(settings: &Settings) -> String {
    toml::to_string(settings).unwrap_or_default()
}

fn invalid(text: &str, error: &toml::de::Error) -> SettingsError {
    let offset = error.span().map_or(0, |span| span.start);
    invalid_value(text, offset, error.message().to_owned())
}

pub(super) fn invalid_value(text: &str, offset: usize, message: String) -> SettingsError {
    let (line, column) = line_and_column(text, offset);
    SettingsError::Invalid {
        line,
        column,
        message,
    }
}

pub(super) fn line_of(text: &str, offset: usize) -> usize {
    line_and_column(text, offset).0
}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

// Dotted paths of every table header and key, in file order (serde_ignored reports them out of order).
fn key_positions(text: &str) -> Vec<String> {
    let mut table = String::new();
    let mut keys = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            header.trim().clone_into(&mut table);
            keys.push(table.clone());
        } else if let Some((key, _)) = line.split_once('=') {
            let key = key.trim();
            keys.push(if table.is_empty() {
                key.to_owned()
            } else {
                format!("{table}.{key}")
            });
        }
    }
    keys
}
