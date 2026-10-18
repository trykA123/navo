use serde_ignored::Path as Ignored;
use toml_edit::DocumentMut;

use super::super::{Settings, SettingsError};
use crate::text_layout::{plain_text, restore_layout};

pub(super) struct Source {
    pub file: toml::Table,
    pub ignored: Vec<Vec<String>>,
}

pub(super) fn without_bom(text: &str) -> String {
    plain_text(text)
}

pub(super) fn edit<T>(
    text: &str,
    change: impl FnOnce(&mut DocumentMut, &Source) -> T,
) -> Result<(String, T), SettingsError> {
    let plain = without_bom(text);
    let source = source(&plain)?;
    let mut document: DocumentMut = plain.parse().map_err(|_| SettingsError::FileInvalid)?;
    let untouched = document.to_string();
    let result = change(&mut document, &source);
    let edited = document.to_string();
    if edited == untouched {
        return Ok((text.to_owned(), result));
    }
    Ok((restore_layout(text, &edited), result))
}

fn source(plain: &str) -> Result<Source, SettingsError> {
    let mut ignored = Vec::new();
    let _: Settings = toml::de::Deserializer::parse(plain)
        .and_then(|de| serde_ignored::deserialize(de, |path| ignored.extend(segments(&path))))
        .map_err(|_| SettingsError::FileInvalid)?;
    let file = toml::from_str(plain).map_err(|_| SettingsError::FileInvalid)?;
    Ok(Source { file, ignored })
}

fn segments(path: &Ignored<'_>) -> Option<Vec<String>> {
    match path {
        Ignored::Root => Some(Vec::new()),
        Ignored::Map { parent, key } => {
            let mut found = segments(parent)?;
            found.push(key.clone());
            Some(found)
        }
        Ignored::Some { parent }
        | Ignored::NewtypeStruct { parent }
        | Ignored::NewtypeVariant { parent } => segments(parent),
        Ignored::Seq { .. } => None,
    }
}
