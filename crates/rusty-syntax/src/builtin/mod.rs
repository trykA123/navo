use crate::{Language, from_toml};

const RULES: [&str; 3] = [
    include_str!("rules/m4.toml"),
    include_str!("rules/oil.toml"),
    include_str!("rules/dcm.toml"),
];

#[must_use]
pub fn builtin() -> Vec<Language> {
    RULES
        .iter()
        .filter_map(|rules| from_toml(rules).ok())
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests;
