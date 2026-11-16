//! The NV side of the compare file seam: builds `CompareFiles` from the viewer's `FileHooks`.

use super::CompareFiles;
use super::FileHooks;

impl From<FileHooks> for CompareFiles {
    /// Keeps only what the compare screen reads: `bytes`, and `paged` narrowed to its source.
    fn from(hooks: FileHooks) -> Self {
        let paged = hooks.paged;
        Self {
            bytes: hooks.bytes,
            source: std::sync::Arc::new(move |location| paged(location).map(|p| p.source)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use navo_core::{Location, ProviderPath};

    use super::*;

    #[test]
    fn compare_files_read_through_the_viewer_hooks() {
        let hooks = FileHooks {
            bytes: Arc::new(|_, _| Ok(vec![7])),
            ..FileHooks::none()
        };
        let files = CompareFiles::from(hooks);
        let here = Location::local(ProviderPath::from_bytes(b"/x")).unwrap();
        assert_eq!((files.bytes)(&here, 8), Ok(vec![7]));
        assert!((files.source)(&here).is_err());
        assert_eq!(
            (CompareFiles::none().bytes)(&here, 1),
            Err("Files cannot be opened here.".to_owned())
        );
    }
}
