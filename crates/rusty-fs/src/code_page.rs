use encoding_rs::{Encoding, WINDOWS_1252};

#[must_use]
pub fn ansi_code_page() -> &'static Encoding {
    WINDOWS_1252
}
