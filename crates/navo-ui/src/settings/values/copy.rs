//! Copy settings read from the form values: when a copy or move verifies its files (AD11a).

use navo_fs::VerifyMode;

use super::Values;
use crate::settings::fields::Field;

/// Verify modes in the order of the `COPY_VERIFY` choices.
const VERIFY_ORDER: [VerifyMode; 3] = [VerifyMode::Auto, VerifyMode::Always, VerifyMode::Off];

impl Values {
    /// The "Verify after copy" choice; an unknown pick reads `Auto`.
    pub fn copy_verify(&self) -> VerifyMode {
        VERIFY_ORDER
            .get(self.pick(Field::CopyVerify))
            .copied()
            .unwrap_or_default()
    }
}
