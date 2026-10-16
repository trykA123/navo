use rusqlite::{OptionalExtension, params};
use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::Needle;
use serde::Serialize;

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

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, DbError> {
    postcard::to_stdvec(value).map_err(|error| DbError::Encode(error.to_string()))
}

const VIEWER_MODULE: &str = "viewer";

fn split_key(extension: &str) -> String {
    format!("markdown-split.{extension}")
}

fn size_of(stamp: Stamp) -> i64 {
    i64::try_from(stamp.size).unwrap_or(i64::MAX)
}

impl Db {
    pub fn viewer_memory(&self, path: &str, stamp: Stamp) -> Result<Option<FileMemory>, DbError> {
        let row = self
            .connection()
            .query_row(
                "SELECT size, modified, marks, bookmarks FROM viewer_memory WHERE path = ?1",
                [path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((size, modified, marks, bookmarks)) = row else {
            return Ok(None);
        };
        let Ok(marks) = postcard::from_bytes::<MarkSet>(&marks) else {
            return Ok(None);
        };
        let Ok(bookmarks) = postcard::from_bytes::<Bookmarks>(&bookmarks) else {
            return Ok(None);
        };
        let fresh = size == size_of(stamp) && modified == stamp.modified;
        Ok(Some(FileMemory {
            marks,
            bookmarks: if fresh {
                bookmarks
            } else {
                Bookmarks::default()
            },
        }))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P15 A5 contract"
    )]
    pub fn save_viewer_memory(
        &self,
        path: &str,
        stamp: Stamp,
        memory: &FileMemory,
        now: i64,
    ) -> Result<(), DbError> {
        if memory.marks.is_empty() && memory.bookmarks.is_empty() {
            self.connection()
                .execute("DELETE FROM viewer_memory WHERE path = ?1", [path])?;
            return Ok(());
        }
        self.connection().execute(
            "INSERT INTO viewer_memory (path, size, modified, marks, bookmarks, saved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (path) DO UPDATE SET size = excluded.size, modified = excluded.modified,
                 marks = excluded.marks, bookmarks = excluded.bookmarks, saved_at = excluded.saved_at",
            params![
                path,
                size_of(stamp),
                stamp.modified,
                encode(&memory.marks)?,
                encode(&memory.bookmarks)?,
                now
            ],
        )?;
        Ok(())
    }

    pub fn search_history(&self) -> Result<SearchHistory, DbError> {
        let mut statement = self
            .connection()
            .prepare("SELECT needle FROM search_history ORDER BY used_at ASC, rowid ASC")?;
        let blobs = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut history = SearchHistory::default();
        for blob in blobs {
            if let Ok(needle) = postcard::from_bytes::<Needle>(&blob) {
                history.push(needle);
            }
        }
        Ok(history)
    }

    pub fn remember_search(&self, needle: &Needle, now: i64) -> Result<(), DbError> {
        if needle.is_empty() {
            return Ok(());
        }
        let conn = self.connection();
        conn.execute(
            "INSERT INTO search_history (needle, used_at) VALUES (?1, ?2)
             ON CONFLICT (needle) DO UPDATE SET used_at = excluded.used_at",
            params![encode(needle)?, now],
        )?;
        conn.execute(
            "DELETE FROM search_history WHERE rowid NOT IN
             (SELECT rowid FROM search_history ORDER BY used_at DESC, rowid DESC LIMIT ?1)",
            [i64::try_from(SearchHistory::MAX).unwrap_or(i64::MAX)],
        )?;
        Ok(())
    }

    pub fn markdown_split(&self, extension: &str) -> Result<Option<bool>, DbError> {
        let value = self
            .connection()
            .query_row(
                "SELECT value FROM module_kv WHERE module = ?1 AND key = ?2",
                [VIEWER_MODULE, &split_key(extension)],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        Ok(value.map(|value| value.first().is_some_and(|byte| *byte != 0)))
    }

    pub fn save_markdown_split(&self, extension: &str, is_shown: bool) -> Result<(), DbError> {
        self.connection().execute(
            "INSERT INTO module_kv (module, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT (module, key) DO UPDATE SET value = excluded.value",
            params![
                VIEWER_MODULE,
                split_key(extension),
                vec![u8::from(is_shown)]
            ],
        )?;
        Ok(())
    }

    pub fn prune_viewer_memory(&self, keep: usize) -> Result<u64, DbError> {
        let removed = self.connection().execute(
            "DELETE FROM viewer_memory WHERE path NOT IN
             (SELECT path FROM viewer_memory ORDER BY saved_at DESC, rowid DESC LIMIT ?1)",
            [i64::try_from(keep).unwrap_or(i64::MAX)],
        )?;
        Ok(u64::try_from(removed).unwrap_or(u64::MAX))
    }
}
