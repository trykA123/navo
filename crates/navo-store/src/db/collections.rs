use rusqlite::{Row, params};

use super::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionKind {
    Search,
    Snapshot,
    Manual,
}

impl CollectionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Snapshot => "snapshot",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub kind: CollectionKind,
    pub query: Option<String>,
    pub base: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct NewCollection<'a> {
    pub name: &'a str,
    pub kind: CollectionKind,
    pub query: Option<&'a str>,
    pub base: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionItem {
    pub location: String,
    pub added_ms: i64,
}

fn validate_name(name: &str) -> Result<(), DbError> {
    if name.trim().is_empty() || name.chars().count() > 60 {
        return Err(DbError::InvalidCollectionName);
    }
    Ok(())
}

fn read_collection(row: &Row<'_>) -> rusqlite::Result<Collection> {
    let kind = match row.get::<_, String>(2)?.as_str() {
        "search" => CollectionKind::Search,
        "snapshot" => CollectionKind::Snapshot,
        "manual" => CollectionKind::Manual,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Collection {
        id: row.get(0)?,
        name: row.get(1)?,
        kind,
        query: row.get(3)?,
        base: row.get(4)?,
        created_ms: row.get(5)?,
        updated_ms: row.get(6)?,
    })
}

impl Db {
    pub fn list_collections(&self) -> Result<Vec<Collection>, DbError> {
        let mut statement = self.connection().prepare(
            "SELECT id, name, kind, query, base, created_ms, updated_ms
             FROM collections ORDER BY name",
        )?;
        Ok(statement
            .query_map([], read_collection)?
            .collect::<Result<_, _>>()?)
    }

    pub fn create_collection(
        &self,
        collection: &NewCollection<'_>,
        now: i64,
    ) -> Result<i64, DbError> {
        validate_name(collection.name)?;
        self.connection().execute(
            "INSERT INTO collections (name, kind, query, base, created_ms, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                collection.name,
                collection.kind.as_str(),
                collection.query,
                collection.base,
                now
            ],
        )?;
        Ok(self.connection().last_insert_rowid())
    }

    pub fn rename_collection(&self, id: i64, name: &str, now: i64) -> Result<(), DbError> {
        validate_name(name)?;
        self.connection().execute(
            "UPDATE collections SET name = ?2, updated_ms = ?3 WHERE id = ?1",
            params![id, name, now],
        )?;
        Ok(())
    }

    pub fn delete_collection(&self, id: i64) -> Result<(), DbError> {
        self.connection()
            .execute("DELETE FROM collections WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn add_collection_items(
        &self,
        id: i64,
        locations: &[&str],
        now: i64,
    ) -> Result<Vec<String>, DbError> {
        let transaction = self.connection().unchecked_transaction()?;
        let mut added = Vec::new();
        for location in locations {
            let count = transaction.execute(
                "INSERT INTO collection_items (collection_id, location, added_ms)
                 VALUES (?1, ?2, ?3) ON CONFLICT (collection_id, location) DO NOTHING",
                params![id, location, now],
            )?;
            if count > 0 {
                added.push((*location).to_owned());
            }
        }
        if !added.is_empty() {
            transaction.execute(
                "UPDATE collections SET updated_ms = ?2 WHERE id = ?1",
                params![id, now],
            )?;
        }
        transaction.commit()?;
        Ok(added)
    }

    pub fn remove_collection_items(
        &self,
        id: i64,
        locations: &[String],
        now: i64,
    ) -> Result<(), DbError> {
        let transaction = self.connection().unchecked_transaction()?;
        let mut removed = 0;
        for location in locations {
            removed += transaction.execute(
                "DELETE FROM collection_items WHERE collection_id = ?1 AND location = ?2",
                params![id, location],
            )?;
        }
        if removed > 0 {
            transaction.execute(
                "UPDATE collections SET updated_ms = ?2 WHERE id = ?1",
                params![id, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn collection_items(&self, id: i64) -> Result<Vec<CollectionItem>, DbError> {
        let mut statement = self.connection().prepare(
            "SELECT location, added_ms FROM collection_items WHERE collection_id = ?1 ORDER BY location",
        )?;
        Ok(statement
            .query_map([id], |row| {
                Ok(CollectionItem {
                    location: row.get(0)?,
                    added_ms: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }
}
