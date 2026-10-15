use crate::Location;

#[allow(dead_code, reason = "stubs until the state is implemented")]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    back: Vec<Location>,
    forward: Vec<Location>,
}

impl History {
    pub const LIMIT: usize = 100;

    #[must_use]
    pub fn back(&self) -> &[Location] {
        todo!()
    }

    #[must_use]
    pub fn forward(&self) -> &[Location] {
        todo!()
    }
}
