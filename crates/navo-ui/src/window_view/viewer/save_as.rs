//! Save conversion: the encoding and line ending the F3 editor writes on the next Ctrl+S.
//! Chosen in the encoding picker; the default keeps what the file was loaded with.
//! Choosing the loaded encoding again converts nothing, so a UTF-8 BOM stays (E7c decision).

use std::sync::Arc;

use navo_core::encoding::{DetectedBy, Encoding, TextEncoding};
use navo_core::line_ends::LineEnds;

/// The line ending a save writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineEnding {
    /// Each line keeps the ending it was loaded with.
    #[default]
    AsLoaded,
    Lf,
    Crlf,
}

/// A pending conversion for the next save; the default converts nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveAs {
    pub encoding: Option<&'static Encoding>,
    pub line_ending: LineEnding,
}

/// Whether `encoding` is UTF-16, which a conversion always writes with a BOM so it reads back.
fn is_utf16(encoding: &'static Encoding) -> bool {
    matches!(encoding.name(), "UTF-16LE" | "UTF-16BE")
}

impl SaveAs {
    /// True when the next save changes the encoding or the line endings.
    #[must_use]
    pub fn is_conversion(self) -> bool {
        self != Self::default()
    }

    /// This plan with an encoding equal to `loaded` dropped, as saving in it converts nothing.
    #[must_use]
    pub fn relative_to(self, loaded: &'static Encoding) -> Self {
        if self.encoding == Some(loaded) {
            Self {
                encoding: None,
                ..self
            }
        } else {
            self
        }
    }

    /// The encoding a save writes for a file loaded as `loaded`. The loaded encoding keeps its
    /// BOM; another one gets a BOM only for UTF-16. A conversion is marked `Chosen`.
    #[must_use]
    pub fn encoding_for(self, loaded: TextEncoding) -> TextEncoding {
        match self.encoding {
            None => loaded,
            Some(encoding) if encoding == loaded.encoding => TextEncoding {
                by: DetectedBy::Chosen,
                ..loaded
            },
            Some(encoding) => TextEncoding {
                encoding,
                bom: is_utf16(encoding),
                by: DetectedBy::Chosen,
            },
        }
    }

    /// The CRLF flag and per-line endings a save writes, from the loaded `crlf` and `ends`.
    /// LF or CRLF drops the per-line endings so every line gets the chosen one.
    #[must_use]
    pub fn endings_for(
        self,
        crlf: bool,
        ends: Option<Arc<LineEnds>>,
    ) -> (bool, Option<Arc<LineEnds>>) {
        match self.line_ending {
            LineEnding::AsLoaded => (crlf, ends),
            LineEnding::Lf => (false, None),
            LineEnding::Crlf => (true, None),
        }
    }
}
