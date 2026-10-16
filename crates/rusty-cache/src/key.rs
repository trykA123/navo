use rusty_core::Location;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub location: Location,
    pub volume_serial: Option<u64>,
}

impl CacheKey {
    #[must_use]
    pub fn new(location: Location, volume_serial: Option<u64>) -> Self {
        Self {
            location,
            volume_serial,
        }
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        size_of::<Self>() + self.location.as_str().len()
    }
}
