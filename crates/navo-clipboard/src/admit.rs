use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::limits::Limits;
use crate::model::{Captured, Content, Entry, Transfer};

/// Why a capture is not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Reason {
    #[error("empty clipboard content")]
    Empty,
    #[error("clipboard text over the size cap")]
    TooLarge,
    #[error("clipboard file list over the count cap")]
    TooManyPaths,
}

/// Keeps or skips a capture. Nothing is trimmed or changed.
pub fn admit(captured: Captured, limits: &Limits) -> Result<Entry, Reason> {
    if captured.content.is_empty() {
        return Err(Reason::Empty);
    }
    match &captured.content {
        Content::Text(text) if text.len() > limits.max_text_bytes => {
            return Err(Reason::TooLarge);
        }
        Content::Paths { paths, .. } if paths.len() > limits.max_paths => {
            return Err(Reason::TooManyPaths);
        }
        _ => {}
    }
    Ok(Entry {
        content: captured.content,
        source_app: captured.source_app,
        first_ms: captured.at_ms,
        last_ms: captured.at_ms,
        uses: 1,
    })
}

/// SHA-256 over a kind byte and the body; source app and time never count.
pub fn content_key(content: &Content) -> [u8; 32] {
    let mut hasher = Sha256::new();
    match content {
        Content::Text(text) => {
            hasher.update([0u8]);
            hasher.update(text.as_bytes());
        }
        Content::Paths { paths, transfer } => {
            hasher.update([1u8]);
            hasher.update([u8::from(*transfer == Transfer::Cut)]);
            for (index, path) in paths.iter().enumerate() {
                if index > 0 {
                    hasher.update([0u8]);
                }
                hasher.update(path.to_string_lossy().as_bytes());
            }
        }
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::limits::{MAX_PATHS, MAX_TEXT_BYTES};

    fn text(body: &str, source: &str, at_ms: i64) -> Captured {
        Captured {
            content: Content::Text(body.to_owned()),
            source_app: Some(source.to_owned()),
            at_ms,
        }
    }

    fn paths(count: usize, transfer: Transfer) -> Content {
        let paths = (0..count).map(|n| PathBuf::from(format!("f{n}"))).collect();
        Content::Paths { paths, transfer }
    }

    fn captured(content: Content) -> Captured {
        Captured {
            content,
            source_app: None,
            at_ms: 0,
        }
    }

    #[test]
    fn whitespace_only_text_is_skipped() {
        let result = admit(text(" \t\r\n ", "a", 1), &Limits::default());
        assert_eq!(result.err(), Some(Reason::Empty));
    }

    #[test]
    fn empty_file_list_is_skipped() {
        let result = admit(captured(paths(0, Transfer::Copy)), &Limits::default());
        assert_eq!(result.err(), Some(Reason::Empty));
    }

    #[test]
    fn text_at_the_cap_is_kept_and_one_byte_over_is_skipped() {
        let limits = Limits::default();
        let at_cap = "a".repeat(MAX_TEXT_BYTES);
        assert!(admit(text(&at_cap, "a", 1), &limits).is_ok());
        let over = "a".repeat(MAX_TEXT_BYTES + 1);
        let result = admit(text(&over, "a", 1), &limits);
        assert_eq!(result.err(), Some(Reason::TooLarge));
    }

    #[test]
    fn paths_over_the_cap_are_skipped_and_at_the_cap_kept() {
        let limits = Limits::default();
        assert!(admit(captured(paths(MAX_PATHS, Transfer::Copy)), &limits).is_ok());
        let result = admit(captured(paths(MAX_PATHS + 1, Transfer::Copy)), &limits);
        assert_eq!(result.err(), Some(Reason::TooManyPaths));
    }

    #[test]
    fn admit_keeps_the_text_untrimmed() {
        let entry = admit(text("  hi \n", "a", 7), &Limits::default()).ok();
        let entry = entry.map(|e| (e.content, e.first_ms, e.last_ms, e.uses));
        assert_eq!(entry, Some((Content::Text("  hi \n".into()), 7, 7, 1)));
    }

    #[test]
    fn copy_and_cut_of_the_same_paths_have_different_keys() {
        let copy = content_key(&paths(3, Transfer::Copy));
        let cut = content_key(&paths(3, Transfer::Cut));
        assert_ne!(copy, cut);
    }

    #[test]
    fn equal_text_has_equal_keys_whatever_the_source_or_time() {
        let a = text("same", "one", 1);
        let b = text("same", "two", 99);
        assert_eq!(content_key(&a.content), content_key(&b.content));
        assert_ne!(
            content_key(&a.content),
            content_key(&Content::Text("other".into()))
        );
    }

    #[test]
    fn text_and_a_single_path_of_the_same_characters_differ() {
        let one = Content::Paths {
            paths: vec![PathBuf::from("x")],
            transfer: Transfer::Copy,
        };
        assert_ne!(content_key(&one), content_key(&Content::Text("x".into())));
    }
}
