pub(crate) const MIGRATIONS: &[&str] = &[V1, V2, V3];

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
