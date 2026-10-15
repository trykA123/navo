use super::LocationError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(id: &str) -> Result<Self, LocationError> {
        todo!()
    }

    #[must_use]
    pub fn local() -> Self {
        todo!()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        todo!()
    }
}
