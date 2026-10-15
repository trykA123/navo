use crate::{Language, RulesError};

pub fn from_udl(_xml: &str) -> Result<Vec<Language>, RulesError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests;
