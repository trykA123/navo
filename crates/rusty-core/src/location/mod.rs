mod path;
mod provider;

pub use path::ProviderPath;
pub use provider::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    provider: ProviderId,
    path: ProviderPath,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocationError {
    #[error("location is empty")]
    Empty,
    #[error("location contains a NUL character")]
    Nul,
    #[error("location names an invalid provider")]
    BadProvider,
}

impl Location {
    pub fn new(text: impl Into<String>) -> Result<Self, LocationError> {
        let (provider, path) = split_text(&text.into())?;
        Self::with_provider(provider, path)
    }

    pub fn local(path: ProviderPath) -> Result<Self, LocationError> {
        todo!()
    }

    pub fn with_provider(provider: ProviderId, path: ProviderPath) -> Result<Self, LocationError> {
        todo!()
    }

    #[must_use]
    pub fn provider(&self) -> &ProviderId {
        todo!()
    }

    #[must_use]
    pub fn path(&self) -> &ProviderPath {
        todo!()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn is_local(&self) -> bool {
        todo!()
    }

    #[must_use]
    pub fn same_place(&self, other: &Self) -> bool {
        todo!()
    }
}

fn split_text(text: &str) -> Result<(ProviderId, ProviderPath), LocationError> {
    todo!()
}

#[cfg(test)]
mod tests;
