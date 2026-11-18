use crate::{Language, from_toml};

const RULES: [&str; 8] = [
    include_str!("rules/a2l.toml"),
    include_str!("rules/dbc.toml"),
    include_str!("rules/dcm.toml"),
    include_str!("rules/ldf.toml"),
    include_str!("rules/lsl.toml"),
    include_str!("rules/m4.toml"),
    include_str!("rules/map.toml"),
    include_str!("rules/oil.toml"),
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
