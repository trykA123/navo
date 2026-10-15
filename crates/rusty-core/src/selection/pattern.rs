#[allow(dead_code, reason = "stubs until the selection is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    text: String,
    items: Vec<Vec<char>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("pattern has no items")]
    Empty,
}

#[allow(unused_variables, reason = "stubs until the selection is implemented")]
impl Pattern {
    pub fn parse(text: &str) -> Result<Self, PatternError> {
        todo!()
    }

    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        todo!()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        todo!()
    }
}
