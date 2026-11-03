use super::{CacheDb, CacheError};

pub const GIT_BADGES_BUDGET: u64 = 2 * 1024 * 1024;

pub(super) const TABLE: &str = "git-badges";

impl CacheDb {
    /// Every saved repository badge as `(repository root, value)`, least recently saved first.
    pub fn git_badges(&self) -> Result<Vec<(String, Vec<u8>)>, CacheError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT key, value FROM blob WHERE tbl = ?1 ORDER BY accessed, key")?;
        let rows = statement.query_map([TABLE], |row| {
            let key: Vec<u8> = row.get(0)?;
            Ok((key, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut found = Vec::new();
        for row in rows {
            let (key, value) = row?;
            if let Ok(root) = String::from_utf8(key) {
                found.push((root, value));
            }
        }
        Ok(found)
    }

    pub fn put_git_badge(&mut self, root: &str, value: &[u8]) -> Result<(), CacheError> {
        self.put(TABLE, root.as_bytes(), value)
    }

    pub fn remove_git_badge(&mut self, root: &str) -> Result<bool, CacheError> {
        self.remove(TABLE, root.as_bytes())
    }
}

pub(super) fn configure(cache: &mut CacheDb) {
    cache.set_budget(TABLE, GIT_BADGES_BUDGET);
}
