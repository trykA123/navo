use crate::{Language, RulesError};

pub fn from_toml(_src: &str) -> Result<Vec<Language>, RulesError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests;
