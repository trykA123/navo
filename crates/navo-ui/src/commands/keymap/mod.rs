mod overlay;
mod parse;
mod shown;

#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use gpui_kit::Keystroke;

use super::CommandSpec;
use super::bind::with_shifted_digit_aliases;
use crate::host_os::HOST_OS;

pub use overlay::{KeymapWarning, Overlaid, UserKeymapError};
#[cfg(test)]
pub(crate) use parse::also_in;
pub use parse::{KeymapError, normal as normal_key};
pub use shown::shown;

/// The compare screen's keys, which Sidewise's keymap includes (NC11).
pub const COMPARE_KEYMAP: &str = include_str!("../compare_keymap.toml");

const BUILTIN: &str = concat!(
    include_str!("../keymap.toml"),
    "\n",
    include_str!("../compare_keymap.toml")
);

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bind {
    pub spec: CommandSpec,
    pub context: String,
    pub keys: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Keymap {
    binds: Vec<Bind>,
    user: Vec<(&'static str, String)>,
    changed: Vec<&'static str>,
}

impl Keymap {
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        parse::parse(text).map(|binds| Self {
            binds,
            ..Self::default()
        })
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

    /// Keys of `command` in one context, after the user overlay.
    pub fn keys_in<'a>(
        &'a self,
        command: &'a str,
        context: &'a str,
    ) -> impl Iterator<Item = &'a str> {
        self.binds
            .iter()
            .filter(move |bind| bind.spec.name == command && bind.context == context)
            .flat_map(|bind| bind.keys.iter().map(String::as_str))
    }

    /// True when `stroke` is bound to `command` in `context`, with the Linux shifted-digit aliases.
    #[must_use]
    pub fn has_key_in(&self, command: &str, context: &str, stroke: &Keystroke) -> bool {
        let wanted = stroke.unparse(HOST_OS);
        self.keys_in(command, context).any(|bound| {
            with_shifted_digit_aliases(bound)
                .iter()
                .any(|key| parse::normal(key).is_some_and(|key| key == wanted))
        })
    }

    /// The first key of `command` in `context`, formatted for display.
    #[must_use]
    pub fn shown_in(&self, command: &str, context: &str) -> Option<String> {
        self.keys_in(command, context).find_map(shown)
    }

    #[must_use]
    pub fn is_user(&self, command: &str, context: &str) -> bool {
        self.user
            .iter()
            .any(|(name, at)| *name == command && at == context)
    }

    #[must_use]
    pub fn is_changed(&self, command: &str) -> bool {
        self.changed.contains(&command)
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

#[cfg(test)]
mod fragment_tests {
    use super::{BUILTIN, COMPARE_KEYMAP, Keymap};

    #[test]
    fn compare_fragment_holds_only_compare_contexts() {
        let keymap = Keymap::parse(COMPARE_KEYMAP).expect("compare keymap");
        assert!(!keymap.binds().is_empty());
        for bind in keymap.binds() {
            assert!(
                bind.context.starts_with("Compare"),
                "{} sits in {}",
                bind.spec.name,
                bind.context
            );
        }
    }

    #[test]
    fn builtin_keymap_parses_with_the_fragment() {
        let keymap = Keymap::parse(BUILTIN).expect("builtin keymap");
        let main = Keymap::parse(include_str!("../keymap.toml")).expect("main keymap");
        let compare = Keymap::parse(COMPARE_KEYMAP).expect("compare keymap");
        assert_eq!(
            keymap.binds().len(),
            main.binds().len() + compare.binds().len()
        );
    }
}
