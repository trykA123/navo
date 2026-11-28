use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::{Hit, SetFilter, TagIndex, search};
use crate::pack::{self, IconDraft, SetDraft};
use crate::registry::{Registry, registry};

const SAMPLE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>"#;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "navo-icons-search-{prefix}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn icon(name: &str, tags: &[&str]) -> IconDraft {
    IconDraft {
        name: name.to_owned(),
        svg: SAMPLE_SVG.to_vec(),
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        brand: None,
    }
}

fn pack_bytes(sets: &[(&str, Vec<IconDraft>)]) -> Vec<u8> {
    let drafts: Vec<SetDraft> = sets
        .iter()
        .map(|(id, icons)| SetDraft {
            id: (*id).to_owned(),
            version: "1.0.0".to_owned(),
            licence: "MIT".to_owned(),
            grid: 24,
            icons: icons.clone(),
        })
        .collect();
    pack::write(&drafts).expect("write test pack")
}

fn registry_from_bytes(bytes: Vec<u8>) -> Registry {
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    Registry::new(leaked).expect("open test registry")
}

fn attach_downloaded(reg: &Registry, set_id: &str, icons: Vec<IconDraft>) {
    let dir = TempDir::new(set_id);
    let bytes = pack_bytes(&[(set_id, icons)]);
    let filename = format!("{set_id}-1.0.0.nvic");
    fs::write(dir.path().join(&filename), &bytes).expect("write downloaded pack");
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let manifest =
        format!("[[set]]\nid = \"{set_id}\"\nfile = \"{filename}\"\nblake3 = \"{hash}\"\n");
    fs::write(dir.path().join("installed.toml"), manifest).expect("write manifest");
    let warnings = reg.load_downloaded(dir.path());
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

#[test]
fn search_exact_name_first() {
    let bytes = pack_bytes(&[(
        "alpha",
        vec![
            icon("bag", &["subfile", "container"]),
            icon("box", &["file-item", "storage"]),
            icon("my-file", &["misc"]),
            icon("file-open", &["misc"]),
            icon("file", &["misc"]),
        ],
    )]);
    let reg = registry_from_bytes(bytes);
    let index = TagIndex::load(&reg);

    let hits = search(&index, "FILE", &SetFilter::All, 10);
    let names: Vec<&str> = hits.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(names, vec!["file", "file-open", "my-file", "box", "bag"]);
}

#[test]
fn search_uses_tags() {
    let bytes = pack_bytes(&[(
        "alpha",
        vec![
            icon("trash", &["delete", "bin", "garbage"]),
            icon("home", &["house", "start"]),
        ],
    )]);
    let reg = registry_from_bytes(bytes);
    let index = TagIndex::load(&reg);

    let hits = search(&index, "garbage", &SetFilter::All, 10);
    assert_eq!(
        hits,
        vec![Hit {
            set: "alpha".to_owned(),
            name: "trash".to_owned(),
        }]
    );
}

#[test]
fn search_every_word_must_match() {
    let bytes = pack_bytes(&[(
        "alpha",
        vec![
            icon("arrow-left", &["direction", "back"]),
            icon("arrow-right", &["direction", "forward"]),
            icon("chevron-left", &["angle", "back"]),
        ],
    )]);
    let reg = registry_from_bytes(bytes);
    let index = TagIndex::load(&reg);

    let hits = search(&index, "arrow back", &SetFilter::All, 10);
    assert_eq!(
        hits,
        vec![Hit {
            set: "alpha".to_owned(),
            name: "arrow-left".to_owned(),
        }]
    );
}

#[test]
fn search_caps_per_set() {
    let bytes = pack_bytes(&[
        (
            "first",
            vec![
                icon("doc-a", &["page"]),
                icon("doc-b", &["page"]),
                icon("doc-c", &["page"]),
                icon("doc-d", &["page"]),
            ],
        ),
        (
            "second",
            vec![
                icon("doc-x", &["page"]),
                icon("doc-y", &["page"]),
                icon("doc-z", &["page"]),
            ],
        ),
    ]);
    let reg = registry_from_bytes(bytes);
    let index = TagIndex::load(&reg);

    let all_hits = search(&index, "doc", &SetFilter::All, 2);
    assert_eq!(
        all_hits,
        vec![
            Hit {
                set: "first".to_owned(),
                name: "doc-a".to_owned(),
            },
            Hit {
                set: "first".to_owned(),
                name: "doc-b".to_owned(),
            },
            Hit {
                set: "second".to_owned(),
                name: "doc-x".to_owned(),
            },
            Hit {
                set: "second".to_owned(),
                name: "doc-y".to_owned(),
            },
        ]
    );

    let set_hits = search(&index, "doc", &SetFilter::Set("first".to_owned()), 2);
    assert_eq!(set_hits.len(), 4);
    assert!(set_hits.iter().all(|hit| hit.set == "first"));

    let empty_query = search(&index, "   ", &SetFilter::All, 1);
    assert_eq!(
        empty_query,
        vec![
            Hit {
                set: "first".to_owned(),
                name: "doc-a".to_owned(),
            },
            Hit {
                set: "second".to_owned(),
                name: "doc-x".to_owned(),
            },
        ]
    );
}

#[test]
fn search_filter_downloaded() {
    let bytes = pack_bytes(&[("bundled", vec![icon("star", &["favorite"])])]);
    let reg = registry_from_bytes(bytes);
    attach_downloaded(&reg, "fluent", vec![icon("star-filled", &["favorite"])]);
    let index = TagIndex::load(&reg);

    let hits = search(&index, "star", &SetFilter::Downloaded, 10);
    assert_eq!(
        hits,
        vec![Hit {
            set: "fluent".to_owned(),
            name: "star-filled".to_owned(),
        }]
    );
}

#[test]
fn search_handles_empty_tags_and_short_tag_lists() {
    let bytes = pack_bytes(&[(
        "sparse",
        vec![
            icon("alpha-one", &[]),
            icon("beta-two", &["first-tag", "second-tag"]),
            icon("gamma-three", &[]),
        ],
    )]);
    let reg = registry_from_bytes(bytes);
    let index = TagIndex::load(&reg);

    let all = search(&index, "", &SetFilter::All, 10);
    let all_names: Vec<&str> = all.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(all_names, vec!["alpha-one", "beta-two", "gamma-three"]);

    let zero_cap = search(&index, "alpha", &SetFilter::All, 0);
    assert!(zero_cap.is_empty());

    let tag_hit = search(&index, "second-tag", &SetFilter::All, 10);
    assert_eq!(
        tag_hit,
        vec![Hit {
            set: "sparse".to_owned(),
            name: "beta-two".to_owned(),
        }]
    );

    let multi_word = search(
        &index,
        "beta second",
        &SetFilter::Set("sparse".to_owned()),
        1,
    );
    assert_eq!(
        multi_word,
        vec![Hit {
            set: "sparse".to_owned(),
            name: "beta-two".to_owned(),
        }]
    );

    let no_match = search(&index, "gamma second", &SetFilter::All, 10);
    assert!(no_match.is_empty());
}

#[test]
fn search_load_budget() {
    let reg = registry();
    let started = Instant::now();
    let index = TagIndex::load(reg);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let _ = writeln!(
        std::io::stdout(),
        "TagIndex::load(registry()): {elapsed_ms:.2} ms across {} sets",
        index.set_count()
    );
    assert!(index.set_count() > 0, "bundled registry should have sets");
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed_ms <= 150.0,
        "TagIndex::load took {elapsed_ms:.2} ms, exceeding 150 ms budget"
    );
}
