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
        Self::with_provider(ProviderId::local(), path)
    }

    pub fn with_provider(provider: ProviderId, path: ProviderPath) -> Result<Self, LocationError> {
        if path.is_empty() {
            return Err(LocationError::Empty);
        }
        if path.has_nul() {
            return Err(LocationError::Nul);
        }
        let text = if provider.is_local() {
            path.to_text()
        } else {
            format!("{}://{}", provider.as_str(), path.to_text())
        };
        Ok(Self {
            provider,
            path,
            text,
        })
    }

    #[must_use]
    pub fn provider_root(provider: ProviderId) -> Self {
        let path = ProviderPath::from_text("/");
        let text = if provider.is_local() {
            path.to_text()
        } else {
            format!("{}:///", provider.as_str())
        };
        Self {
            provider,
            path,
            text,
        }
    }

    #[must_use]
    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    #[must_use]
    pub fn path(&self) -> &ProviderPath {
        &self.path
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn is_local(&self) -> bool {
        self.provider.is_local()
    }

    #[must_use]
    pub fn same_place(&self, other: &Self) -> bool {
        if self.provider != other.provider {
            return false;
        }
        if self.is_local() && windows_style(&self.text) {
            self.path.windows_eq(&other.path)
        } else {
            self.path == other.path
        }
    }
}

fn split_text(text: &str) -> Result<(ProviderId, ProviderPath), LocationError> {
    match text.split_once("://") {
        Some((prefix, rest)) => match ProviderId::new(prefix) {
            Ok(id) if id.is_local() => Err(LocationError::BadProvider),
            Ok(id) => Ok((id, ProviderPath::from_text(rest))),
            Err(_) => Ok((ProviderId::local(), ProviderPath::from_text(text))),
        },
        None => Ok((ProviderId::local(), ProviderPath::from_text(text))),
    }
}

fn windows_style(text: &str) -> bool {
    let mut chars = text.chars();
    text.starts_with(r"\\")
        || matches!((chars.next(), chars.next()), (Some(letter), Some(':')) if letter.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests;
