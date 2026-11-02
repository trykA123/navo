use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, DefaultHasher, Hash, Hasher};

#[derive(Clone)]
pub struct Salt([u8; 16]);

impl Salt {
    #[must_use]
    pub fn random() -> Self {
        let state = RandomState::new();
        let mut bytes = [0; 16];
        let (first, second) = bytes.split_at_mut(8);
        first.copy_from_slice(&state.hash_one(0_u8).to_le_bytes());
        second.copy_from_slice(&state.hash_one(1_u8).to_le_bytes());
        Self(bytes)
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub(super) fn token(&self, segment: &str) -> String {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        segment.to_lowercase().hash(&mut hasher);
        format!("{:04x}", hasher.finish() & 0xffff)
    }
}

impl fmt::Debug for Salt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Salt(..)")
    }
}
