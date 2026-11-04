use serde::Serialize;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct FoldedExtension(String);

impl FoldedExtension {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn fold_extension(extension: &str, files: u64, folders: u64) -> FoldedExtension {
    let allowed = files >= 10
        && folders >= 2
        && !extension.is_empty()
        && extension.len() <= 8
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric());
    FoldedExtension(if allowed {
        extension.to_ascii_lowercase()
    } else {
        "other".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_count_boundary() {
        for (files, expected) in [(9, "other"), (10, "txt"), (11, "txt")] {
            assert_eq!(fold_extension("txt", files, 2).0, expected);
        }
    }

    #[test]
    fn folder_count_boundary() {
        for (folders, expected) in [(1, "other"), (2, "txt"), (3, "txt")] {
            assert_eq!(fold_extension("txt", 10, folders).0, expected);
        }
    }

    #[test]
    fn length_boundary() {
        for (extension, expected) in [
            ("abcdefg", "abcdefg"),
            ("abcdefgh", "abcdefgh"),
            ("abcdefghi", "other"),
        ] {
            assert_eq!(fold_extension(extension, 10, 2).0, expected);
        }
    }

    #[test]
    fn only_nonempty_ascii_alphanumerics_survive() {
        for extension in ["", "résumé", "txt-x", ".txt", "<script>", "a_b", "\0"] {
            assert_eq!(fold_extension(extension, 10, 2).0, "other");
        }
        assert_eq!(fold_extension("MP4", 10, 2).0, "mp4");
        assert_eq!(fold_extension("txt", 0, 0).0, "other");
    }
}
