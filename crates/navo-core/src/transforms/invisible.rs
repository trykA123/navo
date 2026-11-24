//! Finds invisible and look-alike characters that can hide in editor text.

/// The family of an invisible or look-alike character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvisibleKind {
    ZeroWidth,
    BidiMark,
    Bom,
    NoBreakSpace,
    SmartQuote,
}

/// One suspicious character found in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invisible {
    pub at: usize,
    pub ch: char,
    pub kind: InvisibleKind,
}

/// Lists suspicious characters in byte order; a BOM at offset zero is ignored.
pub fn scan_invisible(text: &str) -> Vec<Invisible> {
    let mut found = Vec::new();
    for (at, ch) in text.char_indices() {
        let kind = match ch {
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' => InvisibleKind::ZeroWidth,
            '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => {
                InvisibleKind::BidiMark
            }
            '\u{FEFF}' if at == 0 => continue,
            '\u{FEFF}' => InvisibleKind::Bom,
            '\u{00A0}' | '\u{202F}' => InvisibleKind::NoBreakSpace,
            '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' => InvisibleKind::SmartQuote,
            _ => continue,
        };
        found.push(Invisible { at, ch, kind });
    }
    found
}
