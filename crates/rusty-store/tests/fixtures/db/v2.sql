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

INSERT INTO history (id, location, visited_at) VALUES
    (1, 'file:///home/admin', 1760500000),
    (2, 'file:///home/admin/Documents', 1760500060),
    (3, 'file:///mnt/data', 1760500120);
INSERT INTO frecency (location, score, last_at) VALUES
    ('file:///home/admin', 12.5, 1760500000),
    ('file:///mnt/data', 3.25, 1760500120);
INSERT INTO tags (location, tag) VALUES
    ('file:///home/admin/Documents', 'work'),
    ('file:///home/admin/Documents', 'red'),
    ('file:///mnt/data', 'archive');
INSERT INTO module_kv (module, key, value) VALUES
    ('git', 'last_repo', X'2f686f6d652f61646d696e'),
    ('preview', 'zoom', X'0a');
INSERT INTO job (id, kind, state, created_at, plan) VALUES
    (1, 'copy', 'done', 1760500200, X'01020304'),
    (2, 'move', 'running', 1760500300, X'05060708');
INSERT INTO job_step (job_id, seq, state, detail) VALUES
    (1, 0, 'done', NULL),
    (1, 1, 'done', X'ff'),
    (2, 0, 'pending', NULL);
INSERT INTO undo (id, job_id, created_at, inverse) VALUES
    (1, 1, 1760500210, X'04030201'),
    (2, NULL, 1760500400, X'aa');

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

INSERT INTO viewer_memory (path, size, modified, marks, bookmarks, saved_at) VALUES
    ('/home/admin/notes.txt', 120, 1760600000, X'00', X'00', 1760600100);
INSERT INTO search_history (needle, used_at) VALUES
    (X'0474657374', 1760600200);

PRAGMA user_version = 2;
