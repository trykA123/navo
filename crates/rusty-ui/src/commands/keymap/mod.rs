mod overlay;
mod parse;
mod shown;

#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use super::CommandSpec;

pub use overlay::{KeymapWarning, Overlaid, UserKeymapError};
pub use parse::{KeymapError, normal as normal_key};
pub use shown::shown;

const BUILTIN: &str = include_str!("../keymap.toml");

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bind {
    pub spec: CommandSpec,
    pub context: String,
    pub keys: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Keymap {
    binds: Vec<Bind>,
}

impl Keymap {
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        parse::parse(text).map(|binds| Self { binds })
    }

    #[must_use]
    pub fn builtin() -> &'static Self {
        static PARSED: OnceLock<Keymap> = OnceLock::new();
        PARSED.get_or_init(|| Self::parse(BUILTIN).unwrap_or_default())
    }

    pub fn overlay(&self, user: &str) -> Result<Overlaid, UserKeymapError> {
        overlay::overlay(self, user)
    }

    #[must_use]
    pub fn binds(&self) -> &[Bind] {
        &self.binds
    }

    pub fn keys_of<'a>(&'a self, command: &'a str) -> impl Iterator<Item = &'a str> {
        self.binds
            .iter()
            .filter(move |bind| bind.spec.name == command)
            .flat_map(|bind| bind.keys.iter().map(String::as_str))
    }

    #[must_use]
    pub fn has_key(&self, command: &str, key: &str) -> bool {
        let wanted = parse::normal(key);
        wanted.is_some()
            && self
                .keys_of(command)
                .any(|bound| parse::normal(bound) == wanted)
    }
}
