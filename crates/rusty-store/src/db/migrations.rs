pub(crate) const MIGRATIONS: &[&str] = &[V1, V2, V3, V4, V5, V6, V7, V8];

const V1: &str = "
CREATE TABLE history (
    id INTEGER PRIMARY KEY,
    location TEXT NOT NULL,
    visited_at INTEGER NOT NULL
);
CREATE TABLE frecency (
    location TEXT PRIMARY KEY,
    score REAL NOT NULL,
    last_at INTEGER NOT NULL
);
CREATE TABLE tags (
    location TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (location, tag)
);
CREATE TABLE module_kv (
    module TEXT NOT NULL,
    key TEXT NOT NULL,
    value BLOB NOT NULL,
    PRIMARY KEY (module, key)
);
CREATE TABLE job (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    plan BLOB NOT NULL
);
CREATE TABLE job_step (
    job_id INTEGER NOT NULL REFERENCES job(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    state TEXT NOT NULL,
    detail BLOB,
    PRIMARY KEY (job_id, seq)
);
CREATE TABLE undo (
    id INTEGER PRIMARY KEY,
    job_id INTEGER REFERENCES job(id),
    created_at INTEGER NOT NULL,
    inverse BLOB NOT NULL
);
";

const V2: &str = "
CREATE TABLE viewer_memory (
    path TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    marks BLOB NOT NULL,
    bookmarks BLOB NOT NULL,
    saved_at INTEGER NOT NULL
);
CREATE INDEX viewer_memory_saved_at ON viewer_memory (saved_at);
CREATE TABLE search_history (
    needle BLOB PRIMARY KEY,
    used_at INTEGER NOT NULL
);
";

const V3: &str = "
CREATE TABLE settings_defaults (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    written_at INTEGER NOT NULL
);
";

const V4: &str = "
ALTER TABLE frecency ADD COLUMN is_folder INTEGER NOT NULL DEFAULT 1;
CREATE TABLE command_use (
    name TEXT PRIMARY KEY,
    score REAL NOT NULL,
    last_at INTEGER NOT NULL
);
";

const V5: &str = "
CREATE TABLE rename_preset (
    name TEXT PRIMARY KEY,
    rules TEXT NOT NULL,
    saved_at INTEGER NOT NULL
);
";

const V6: &str = "
CREATE TABLE retained_entry (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER REFERENCES job(id) ON DELETE SET NULL,
    state TEXT NOT NULL CHECK (state IN ('preparing', 'ready', 'restoring', 'restored', 'purging', 'purged', 'aborted', 'conflict')),
    record BLOB NOT NULL,
    target BLOB
);
CREATE INDEX retained_entry_job ON retained_entry (job_id);
";

const V7: &str = "
ALTER TABLE retained_entry ADD COLUMN source_path TEXT GENERATED ALWAYS AS
    (CASE WHEN json_valid(record) THEN json_extract(record, '$.source') END) VIRTUAL;
ALTER TABLE retained_entry ADD COLUMN container_path TEXT GENERATED ALWAYS AS
    (CASE WHEN json_valid(record) THEN json_extract(record, '$.container') END) VIRTUAL;
ALTER TABLE retained_entry ADD COLUMN volume_key TEXT GENERATED ALWAYS AS
    (CASE WHEN json_valid(record) THEN record -> '$.parent_identity.volume' END) VIRTUAL;
ALTER TABLE retained_entry ADD COLUMN cleanup_pending INTEGER NOT NULL DEFAULT 1
    CHECK (cleanup_pending IN (0, 1) AND (cleanup_pending = 1 OR state IN ('restored', 'purged', 'aborted')));
ALTER TABLE retained_entry ADD COLUMN cleanup_round INTEGER NOT NULL DEFAULT 0
    CHECK (typeof(cleanup_round) = 'integer' AND cleanup_round >= 0);
CREATE INDEX retained_entry_live_container ON retained_entry (container_path)
    WHERE state NOT IN ('restored', 'purged', 'aborted');
CREATE INDEX retained_entry_cleanup ON retained_entry (volume_key, cleanup_round, id)
    WHERE cleanup_pending = 1 AND state IN ('restored', 'purged', 'aborted');
";

const V8: &str = "
CREATE TABLE collections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE CHECK (length(name) BETWEEN 1 AND 60 AND length(trim(name)) > 0),
    kind TEXT NOT NULL CHECK (kind IN ('search', 'snapshot', 'manual')),
    query TEXT,
    base TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
);
CREATE TABLE collection_items (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    location TEXT NOT NULL,
    added_ms INTEGER NOT NULL,
    PRIMARY KEY (collection_id, location)
);
";
