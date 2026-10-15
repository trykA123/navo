use super::{Settings, SettingsError};

pub fn parse(text: &str) -> Result<(Settings, Vec<String>), SettingsError> {
    let _ = text;
    todo!()
}

#[must_use]
pub fn to_toml(settings: &Settings) -> String {
    let _ = settings;
    todo!()
}
