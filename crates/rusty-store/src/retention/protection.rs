use rusty_core::{AncestorMemo, Location, ProviderPath};

use super::{RetentionCatalog, RetentionStoreError};

const LIVE: &str =
    "SELECT container_path FROM retained_entry INDEXED BY retained_entry_live_container
    WHERE state NOT IN ('restored', 'purged', 'aborted')";

impl RetentionCatalog {
    pub fn has_live(&self) -> Result<bool, RetentionStoreError> {
        Ok(self.db.connection().query_row(
            &format!("SELECT EXISTS({LIVE} LIMIT 1)"),
            [],
            |row| row.get(0),
        )?)
    }

    pub fn intersects_live(&self, paths: &[Location]) -> Result<bool, RetentionStoreError> {
        if paths.is_empty() {
            return Ok(false);
        }
        let parents: Vec<_> = paths.iter().map(Location::parent).collect();
        let mut statement = self.db.connection().prepare(LIVE)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let container: String = row.get(0)?;
            let container = Location::new(container).map_err(|_| {
                rusty_core::retention::RetentionError::Invalid("invalid container projection")
            })?;
            let below: Vec<_> =
                std::iter::successors(Some(container.clone()), Location::parent).collect();
            let mut above = AncestorMemo::new(|level: &Location, _: Option<&ProviderPath>| {
                level.same_place(&container)
            });
            if paths.iter().zip(&parents).any(|(path, parent)| {
                path.same_place(&container)
                    || parent.as_ref().is_some_and(|parent| above.any(parent))
                    || below.iter().any(|level| level.same_place(path))
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intersects(a: &Location, b: &Location) -> bool {
        std::iter::successors(Some(a.clone()), Location::parent).any(|path| path.same_place(b))
            || std::iter::successors(Some(b.clone()), Location::parent)
                .any(|path| path.same_place(a))
    }

    #[test]
    fn grouped_paths_match_individual_intersections_without_refusing_siblings() {
        let catalog = RetentionCatalog::new(crate::Db::open_in_memory().unwrap()).unwrap();
        for text in ["/files/one/.navo-held-ab", r"C:\Données\.navo-held-ab"] {
            catalog
                .db
                .connection()
                .execute("DELETE FROM retained_entry", [])
                .unwrap();
            let record = serde_json::json!({ "container": text });
            catalog
                .db
                .connection()
                .execute(
                    "INSERT INTO retained_entry (state, record) VALUES ('ready', ?1)",
                    [serde_json::to_vec(&record).unwrap()],
                )
                .unwrap();
            let container = Location::new(text).unwrap();
            let paths: Vec<_> = [
                "/files/one/a",
                "/files/one/b",
                "/files/two/a",
                "/files/one",
                "/files/one/.navo-held-ab/payload/a",
                "/files/one/.navo-held-ab",
                "/",
                "/files/one/lookalike",
                r"c:\DONNÉES\a",
                r"c:\DONNÉES\b",
                r"c:\DONNÉES",
                r"c:\DONNÉES\.NAVO-HELD-ab\payload",
                r"C:\",
            ]
            .into_iter()
            .map(|text| Location::new(text).unwrap())
            .collect();
            let batches = (0..paths.len())
                .flat_map(|first| (first + 1..=paths.len()).map(move |last| first..last));
            for batch in batches.map(|range| &paths[range]) {
                assert_eq!(
                    catalog.intersects_live(batch).unwrap(),
                    batch.iter().any(|path| intersects(path, &container)),
                    "{batch:?}"
                );
            }
        }
    }

    #[test]
    fn live_existence_uses_the_v7_index_without_decoding_records() {
        let catalog = RetentionCatalog::new(crate::Db::open_in_memory().unwrap()).unwrap();
        assert!(!catalog.has_live().unwrap());
        let conn = catalog.db.connection();
        conn.execute(
            "INSERT INTO retained_entry (state, record) VALUES ('purged', X'ff')",
            [],
        )
        .unwrap();
        assert!(!catalog.has_live().unwrap());
        conn.execute(
            "INSERT INTO retained_entry (state, record) VALUES ('ready', X'ff')",
            [],
        )
        .unwrap();
        assert!(catalog.has_live().unwrap());
        let detail: String = conn
            .query_row(
                &format!("EXPLAIN QUERY PLAN SELECT EXISTS({LIVE} LIMIT 1)"),
                [],
                |row| row.get(3),
            )
            .unwrap();
        let mut statement = conn
            .prepare(&format!("EXPLAIN QUERY PLAN SELECT EXISTS({LIVE} LIMIT 1)"))
            .unwrap();
        let details = statement
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("retained_entry_live_container")),
            "{detail}: {details:?}"
        );
    }

    #[test]
    fn terminal_manifests_are_never_decoded_and_windows_ancestors_keep_case_rules() {
        let catalog = RetentionCatalog::new(crate::Db::open_in_memory().unwrap()).unwrap();
        let conn = catalog.db.connection();
        conn.execute(
            "INSERT INTO retained_entry (state, record) VALUES ('purged', X'ff')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO retained_entry (state, record) VALUES ('ready', ?1)",
            [
                r#"{"container":"C:\\Données\\.ferrite-held-a","nodes":"not a manifest"}"#
                    .as_bytes(),
            ],
        )
        .unwrap();
        assert!(
            catalog
                .intersects_live(&[Location::new(r"c:\DONNÉES").unwrap()])
                .unwrap()
        );
        assert!(
            !catalog
                .intersects_live(&[Location::new("/other").unwrap()])
                .unwrap()
        );
        let detail: String = conn
            .query_row(&format!("EXPLAIN QUERY PLAN {LIVE}"), [], |row| row.get(3))
            .unwrap();
        assert!(detail.contains("retained_entry_live_container"), "{detail}");
    }
}

#[cfg(test)]
mod stress {
    use super::*;

    #[test]
    #[ignore = "writes 1,000 terminal snapshots with 50,000 nodes each; run explicitly"]
    fn terminal_catalog_size_does_not_enter_the_mutation_guard() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large.db");
        let (db, _) = crate::Db::open(&path, false).unwrap();
        let bytes = snapshot();
        let conn = db.connection();
        conn.execute_batch("PRAGMA synchronous=OFF; BEGIN IMMEDIATE;")
            .unwrap();
        let mut insert = conn.prepare("INSERT INTO retained_entry (state, record, cleanup_pending) VALUES ('purged', ?1, 0)").unwrap();
        for _ in 0..1_000 {
            insert.execute([&bytes]).unwrap();
        }
        drop(insert);
        conn.execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE); PRAGMA synchronous=FULL;")
            .unwrap();
        let catalog = RetentionCatalog::new(db).unwrap();
        let paths = [Location::new("/source/new.txt").unwrap()];
        let start = std::time::Instant::now();
        for _ in 0..100 {
            assert!(!catalog.intersects_live(&paths).unwrap());
        }
        let warm = start.elapsed();
        let fresh = fresh_connections(&path, &paths);
        let report = format!(
            "guard: 1,000 terminal rows x 50,000 nodes; snapshot {} bytes; 100 warm checks {warm:?}; 100 fresh connection checks {fresh:?}; database {} bytes\n",
            bytes.len(),
            std::fs::metadata(&path).unwrap().len()
        );
        let output = std::env::var_os("NAVO_GUARD_BENCHMARK_OUTPUT").unwrap();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap()
            .write_all(report.as_bytes())
            .unwrap();
    }
    fn fresh_connections(path: &std::path::Path, paths: &[Location]) -> std::time::Duration {
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let (db, _) = crate::Db::open(path, false).unwrap();
            assert!(
                !RetentionCatalog::new(db)
                    .unwrap()
                    .intersects_live(paths)
                    .unwrap()
            );
        }
        start.elapsed()
    }

    fn snapshot() -> Vec<u8> {
        use rusty_core::retention::{DAY_MS, RetainedKind, RetainedNode, RetentionRecord};
        let identity = rusty_core::FileIdentity {
            volume: 1,
            file: [1; 16],
            kind: Default::default(),
        };
        let nodes = (0..50_000)
            .map(|index| RetainedNode {
                path: if index == 0 {
                    vec![]
                } else {
                    vec![format!("file-{index}")]
                },
                kind: if index == 0 {
                    RetainedKind::Folder
                } else {
                    RetainedKind::File
                },
                identity,
                size: if index == 0 { 0 } else { 1 },
                modified_ms: Some(1),
            })
            .collect();
        let record = RetentionRecord {
            version: 1,
            payload_name: None,
            source: Location::new("/source/tree").unwrap(),
            container: Location::new("/source/.ferrite-retained-ab").unwrap(),
            parent_identity: identity,
            container_identity: Some(identity),
            nodes,
            created_at: 0,
            expires_at: DAY_MS,
        };
        record.validate().unwrap();
        serde_json::to_vec(&record).unwrap()
    }
}
