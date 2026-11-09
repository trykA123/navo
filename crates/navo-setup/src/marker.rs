//! Bounded navo-install.json serialization for the installer's persistent identity.

use serde::{Deserialize, Serialize};

use crate::mode::ModeError;

#[cfg(test)]
#[path = "marker_tests.rs"]
mod tests;

/// A marker identifies the package and retains the install id across upgrades.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Marker {
    pub pack_id: String,
    pub install_id: String,
    pub created_by: String,
}

impl Marker {
    /// Parses at most 4 KiB of JSON; invalid fields and identities are errors.
    pub fn parse(text: &str) -> Result<Self, ModeError> {
        if text.len() > 4 * 1024 {
            return Err(ModeError::TooLarge);
        }
        let marker: Self = serde_json::from_str(text).map_err(|_| ModeError::Marker)?;
        marker.validate()?;
        Ok(marker)
    }

    /// Renders valid marker JSON, refusing an invalid identity or more than 4 KiB.
    pub fn render(&self) -> Result<String, ModeError> {
        self.validate()?;
        let text = serde_json::to_string(self).map_err(|_| ModeError::Marker)?;
        if text.len() > 4 * 1024 {
            return Err(ModeError::TooLarge);
        }
        Ok(text)
    }

    /// Checks the package id, 32 hexadecimal install-id bytes and nonempty creator.
    fn validate(&self) -> Result<(), ModeError> {
        if self.pack_id != "NavoApp"
            || self.install_id.len() != 32
            || !self.install_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.created_by.trim().is_empty()
        {
            return Err(ModeError::Marker);
        }
        Ok(())
    }
}
