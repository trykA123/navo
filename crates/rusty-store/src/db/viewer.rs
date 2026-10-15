use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::Needle;

use super::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileMemory {
    pub marks: MarkSet,
    pub bookmarks: Bookmarks,
}

impl Db {
    pub fn viewer_memory(&self, _path: &str, _stamp: Stamp) -> Result<Option<FileMemory>, DbError> {
        Ok(None)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P15 A5 contract"
    )]
    pub fn save_viewer_memory(
        &self,
        _path: &str,
        _stamp: Stamp,
        _memory: &FileMemory,
        _now: i64,
    ) -> Result<(), DbError> {
        Ok(())
    }

    pub fn search_history(&self) -> Result<SearchHistory, DbError> {
        Ok(SearchHistory::default())
    }

    pub fn remember_search(&self, _needle: &Needle, _now: i64) -> Result<(), DbError> {
        Ok(())
    }

    pub fn prune_viewer_memory(&self, _keep: usize) -> Result<u64, DbError> {
        Ok(0)
    }
}
