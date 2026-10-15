use super::LocationError;

const LOCAL: &str = "local";
const MAX_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(id: &str) -> Result<Self, LocationError> {
        let mut chars = id.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if first_ok && rest_ok && (2..=MAX_LEN).contains(&id.len()) {
            Ok(Self(id.to_owned()))
        } else {
            Err(LocationError::BadProvider)
        }
    }

    #[must_use]
    pub fn local() -> Self {
        Self(LOCAL.to_owned())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn is_local(&self) -> bool {
        self.0 == LOCAL
    }
}
