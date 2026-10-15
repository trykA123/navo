use super::{Settings, SettingsError};

pub fn parse(text: &str) -> Result<(Settings, Vec<String>), SettingsError> {
    let mut unknown = Vec::new();
    let settings = toml::de::Deserializer::parse(text)
        .and_then(|de| serde_ignored::deserialize(de, |path| unknown.push(path.to_string())))
        .map_err(|e| invalid(text, &e))?;
    let order = key_positions(text);
    unknown.sort_by_key(|key| order.iter().position(|k| k == key).unwrap_or(usize::MAX));
    Ok((settings, unknown))
}

#[must_use]
pub fn to_toml(settings: &Settings) -> String {
    try_to_toml(settings).unwrap_or_default()
}

pub(super) fn try_to_toml(settings: &Settings) -> Result<String, toml::ser::Error> {
    toml::to_string(settings)
}

fn invalid(text: &str, error: &toml::de::Error) -> SettingsError {
    let offset = error.span().map_or(0, |span| span.start);
    let (line, column) = line_and_column(text, offset);
    SettingsError::Invalid {
        line,
        column,
        message: error.message().to_owned(),
    }
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
            table = header.trim().to_owned();
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
