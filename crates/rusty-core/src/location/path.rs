use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderPath(Repr);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Repr {
    Text(String),
    Wide(Box<[u16]>),
    Bytes(Box<[u8]>),
}

impl ProviderPath {
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        todo!()
    }

    #[must_use]
    pub fn from_wide(units: &[u16]) -> Self {
        todo!()
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        todo!()
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        todo!()
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        todo!()
    }

    #[must_use]
    pub fn to_wide(&self) -> Option<Vec<u16>> {
        todo!()
    }

    #[must_use]
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        todo!()
    }

    #[must_use]
    pub fn display(&self) -> Cow<'_, str> {
        todo!()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    #[must_use]
    pub fn windows_eq(&self, other: &Self) -> bool {
        todo!()
    }
}
