mod parse;
mod shown;

#[cfg(test)]
mod tests;

use super::CommandSpec;

pub use parse::KeymapError;
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
        let _ = text;
        todo!()
    }

    #[must_use]
    pub fn builtin() -> &'static Self {
        todo!()
    }

    #[must_use]
    pub fn binds(&self) -> &[Bind] {
        &self.binds
    }

    pub fn keys_of<'a>(&'a self, command: &'a str) -> impl Iterator<Item = &'a str> {
        let _ = command;
        std::iter::empty::<&str>().chain(std::iter::once(todo!()))
    }
}
