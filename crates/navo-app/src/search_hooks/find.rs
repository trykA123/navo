//! Find results collection hooks backed by the app's database.

use std::sync::Arc;

use navo_store::{Collection, CollectionKind, Db, NewCollection};
use navo_ui::window_view::{FindHooks, SavedSearch};

use crate::files::{SharedDb, lock, now_ms};

fn to_saved_search(collection: Collection) -> SavedSearch {
    SavedSearch {
        id: collection.id,
        name: collection.name,
        query: collection.query.unwrap_or_default(),
        base: collection.base,
    }
}

fn create_snapshot(
    db: &Db,
    collection: &NewCollection<'_>,
    paths: &[String],
) -> Result<i64, String> {
    let now = now_ms();
    let id = db
        .create_collection(collection, now)
        .map_err(|error| error.to_string())?;
    let items: Vec<&str> = paths.iter().map(String::as_str).collect();
    if let Err(error) = db.add_collection_items(id, &items, now) {
        let _ = db.delete_collection(id);
        return Err(error.to_string());
    }
    Ok(id)
}

/// Builds find-results hooks backed by the collections tables in `db`.
pub(super) fn hooks(db: SharedDb) -> FindHooks {
    let (save_search_db, save_snapshot_db, saved_db, delete_db) =
        (db.clone(), db.clone(), db.clone(), db);
    FindHooks {
        pin: super::pin::order(),
        ensure_owner: Arc::new(super::pin::ensure_owner),
        save_search: Arc::new(move |name, query, base| {
            lock(&save_search_db)
                .create_collection(
                    &NewCollection {
                        name,
                        kind: CollectionKind::Search,
                        query: Some(query),
                        base,
                    },
                    now_ms(),
                )
                .map_err(|error| error.to_string())
        }),
        save_snapshot: Arc::new(move |name, query, base, paths| {
            let collection = NewCollection {
                name,
                kind: CollectionKind::Snapshot,
                query: Some(query),
                base,
            };
            create_snapshot(&lock(&save_snapshot_db), &collection, paths)
        }),
        saved: Arc::new(move || {
            lock(&saved_db)
                .list_collections()
                .inspect_err(|error| tracing::warn!(%error, "loading saved searches failed"))
                .unwrap_or_default()
                .into_iter()
                .filter(|collection| collection.kind == CollectionKind::Search)
                .map(to_saved_search)
                .collect()
        }),
        delete_saved: Arc::new(move |id| {
            lock(&delete_db)
                .delete_collection(id)
                .map_err(|error| error.to_string())
        }),
    }
}
