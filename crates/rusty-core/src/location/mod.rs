#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocationError {
    #[error("location is empty")]
    Empty,
    #[error("location contains a NUL character")]
    Nul,
}

impl Location {
    pub fn new(text: impl Into<String>) -> Result<Self, LocationError> {
        let text = text.into();
        if text.is_empty() {
            return Err(LocationError::Empty);
        }
        if text.contains('\0') {
            return Err(LocationError::Nul);
        }
        Ok(Self(text))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests;
