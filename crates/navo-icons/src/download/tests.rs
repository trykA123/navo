use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use super::{
    DownloadSpec, InstallError, downloads, install, install_with, remove, remove_with,
    validate_pack_with_budget,
};
use crate::pack::{self, IconDraft, PackError, SetDraft};
use crate::registry::Registry;

const SAMPLE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>"#;
const TEXT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><text x="0" y="12">hi</text><path d="M0 0h24v24H0z"/></svg>"#;
const LICENCE: &[u8] = b"MIT License\n\nPermission is hereby granted, free of charge.\n";
const VERSION: &str = "1.0.0";

static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "navo-icons-download-{prefix}-{}-{id}",
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

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn spec_for(id: &str, pack_bytes: &[u8], licence_bytes: &[u8]) -> DownloadSpec {
    DownloadSpec {
        id: id.to_owned(),
        package: "@example/icons".to_owned(),
        version: VERSION.to_owned(),
        pack_path: format!("/releases/{id}.nvic"),
        pack_size: pack_bytes.len() as u64,
        pack_sha256: sha256(pack_bytes),
        licence: "MIT".to_owned(),
        licence_path: format!("/releases/{id}-LICENSE"),
        licence_size: licence_bytes.len() as u64,
        licence_sha256: sha256(licence_bytes),
    }
}

fn icon(name: &str, svg: &[u8]) -> IconDraft {
    IconDraft {
        name: name.to_owned(),
        svg: svg.to_vec(),
        tags: vec!["test".to_owned()],
        brand: None,
    }
}

fn pack_with(id: &str, icons: Vec<IconDraft>) -> Vec<u8> {
    let draft = SetDraft {
        id: id.to_owned(),
        version: VERSION.to_owned(),
        licence: "MIT".to_owned(),
        grid: 24,
        icons,
    };
    pack::write(&[draft]).expect("write test pack")
}

#[test]
fn download_refuses_wrong_size_before_hashing() {
    let dir = TempDir::new("size");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let mut spec = spec_for("sample", &pack_bytes, LICENCE);
    spec.pack_size += 1;
    spec.pack_sha256 = [0; 32];

    let error = install(&pack_bytes, LICENCE, &spec, dir.path()).expect_err("wrong size must fail");
    assert_eq!(
        error,
        InstallError::Size {
            want: spec.pack_size,
            got: pack_bytes.len() as u64,
        }
    );
    assert!(!dir.path().join("installed.toml").exists());
}

#[test]
fn download_refuses_wrong_hash_before_unpacking() {
    let dir = TempDir::new("hash");
    let original = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let spec = spec_for("sample", &original, LICENCE);
    let mut corrupt = original.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0xFF;

    let error = install(&corrupt, LICENCE, &spec, dir.path()).expect_err("bad hash must fail");
    assert_eq!(error, InstallError::Hash);
    assert!(!dir.path().join("installed.toml").exists());
}

#[test]
fn download_refuses_wrong_licence_hash() {
    let dir = TempDir::new("licence");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let mut spec = spec_for("sample", &pack_bytes, LICENCE);
    spec.licence_sha256 = [0; 32];

    let error =
        install(&pack_bytes, LICENCE, &spec, dir.path()).expect_err("bad licence must fail");
    assert_eq!(error, InstallError::Hash);
    assert!(!dir.path().join("installed.toml").exists());
}

#[test]
fn download_revalidates_every_svg_entry() {
    let dir = TempDir::new("svg");
    let pack_bytes = pack_with("sample", vec![icon("bad", TEXT_SVG)]);
    let spec = spec_for("sample", &pack_bytes, LICENCE);

    let error = install(&pack_bytes, LICENCE, &spec, dir.path()).expect_err("text must fail");
    assert!(matches!(error, InstallError::Svg { ref name, .. } if name == "bad"));
    assert_eq!(fs::read_dir(dir.path()).expect("read dir").count(), 0);
}

#[test]
fn download_installs_and_records_blake3() {
    let dir = TempDir::new("install");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let spec = spec_for("sample", &pack_bytes, LICENCE);

    let installed = install(&pack_bytes, LICENCE, &spec, dir.path()).expect("install");
    assert_eq!(installed, dir.path().join("sample-1.0.0.nvic"));
    assert!(installed.exists());
    assert!(dir.path().join("sample-LICENSE").exists());

    let manifest = fs::read_to_string(dir.path().join("installed.toml")).expect("manifest");
    let digest = blake3::hash(&pack_bytes).to_hex();
    assert!(manifest.contains("sample-1.0.0.nvic"));
    assert!(manifest.contains(digest.as_str()));

    let warnings = Registry::empty().load_downloaded(dir.path());
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

#[test]
fn download_remove_updates_installed() {
    let dir = TempDir::new("remove");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let spec = spec_for("sample", &pack_bytes, LICENCE);
    let installed = install(&pack_bytes, LICENCE, &spec, dir.path()).expect("install");

    remove("sample", dir.path()).expect("remove");

    assert!(!installed.exists());
    assert!(!dir.path().join("sample-LICENSE").exists());
    let manifest = fs::read_to_string(dir.path().join("installed.toml")).expect("manifest");
    assert!(!manifest.contains("sample"));
}

#[test]
fn download_sources_toml_lists_four_sets() {
    let ids: Vec<String> = downloads().into_iter().map(|spec| spec.id).collect();
    assert_eq!(ids, ["fluent", "fluent-filled", "material", "carbon"]);
}

#[test]
fn download_install_with_uses_injected_writer() {
    let dir = TempDir::new("inject");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let spec = spec_for("sample", &pack_bytes, LICENCE);
    let calls = RefCell::new(Vec::new());
    let writer = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        calls.borrow_mut().push((path.to_path_buf(), bytes.len()));
        Ok(())
    };

    let installed =
        install_with(&pack_bytes, LICENCE, &spec, dir.path(), &writer).expect("install_with");

    let calls = calls.into_inner();
    assert_eq!(calls.len(), 3);
    assert!(
        calls
            .iter()
            .any(|(path, len)| path == &installed && *len == pack_bytes.len())
    );
    assert!(
        calls
            .iter()
            .any(|(path, len)| path.ends_with("sample-LICENSE") && *len == LICENCE.len())
    );
    assert!(
        calls
            .iter()
            .any(|(path, _)| path.ends_with("installed.toml"))
    );
}

#[test]
fn download_refuses_oversize_manifest() {
    let dir = TempDir::new("oversize-manifest");
    let big = vec![b'#'; 1024 * 1024 + 1];
    fs::write(dir.path().join("installed.toml"), big).expect("write oversize manifest");

    let err = remove("sample", dir.path()).expect_err("oversize manifest must be refused");
    assert!(
        matches!(err, InstallError::Io(_)),
        "expected InstallError::Io for oversize manifest, got {err:?}"
    );
}

#[test]
fn download_refuses_oversize_body_over_64kib() {
    let dir = TempDir::new("oversize-body");
    let prefix =
        br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/>"#;
    let suffix = b"</svg>";
    let pad = 64 * 1024 + 1 - prefix.len() - suffix.len();
    let mut big_svg = Vec::with_capacity(64 * 1024 + 1);
    big_svg.extend_from_slice(prefix);
    big_svg.resize(prefix.len() + pad, b' ');
    big_svg.extend_from_slice(suffix);
    assert_eq!(big_svg.len(), 64 * 1024 + 1);

    let pack_bytes = pack_with("sample", vec![icon("huge", &big_svg)]);
    let spec = spec_for("sample", &pack_bytes, LICENCE);

    let err = install(&pack_bytes, LICENCE, &spec, dir.path())
        .expect_err("download icon body over 64 KiB must be refused");
    assert_eq!(err, InstallError::Pack(PackError::TooLarge));
    assert_eq!(fs::read_dir(dir.path()).expect("read dir").count(), 0);
}

#[test]
fn download_refuses_leading_dot_id() {
    let dir = TempDir::new("leading-dot");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let mut spec = spec_for("sample", &pack_bytes, LICENCE);
    spec.id = ".hidden".to_owned();

    let err = install(&pack_bytes, LICENCE, &spec, dir.path())
        .expect_err("leading-dot id must be refused");
    assert!(
        matches!(err, InstallError::Io(ref msg) if msg.contains("unsafe download id")),
        "expected unsafe download id error, got {err:?}"
    );
    assert_eq!(fs::read_dir(dir.path()).expect("read dir").count(), 0);
}

#[test]
fn download_remove_refuses_mismatched_file_and_keeps_other_pack() {
    let dir = TempDir::new("mismatched-remove");
    let other_pack = dir.path().join("beta-1.0.0.nvic");
    fs::write(&other_pack, b"keep me").expect("write other pack");
    let manifest = concat!(
        "[[set]]\n",
        "id = \"alpha\"\n",
        "file = \"beta-1.0.0.nvic\"\n",
        "blake3 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
    );
    fs::write(dir.path().join("installed.toml"), manifest).expect("write manifest");

    let err = remove("alpha", dir.path()).expect_err("mismatched file must be refused");
    assert!(matches!(err, InstallError::Io(_)));
    assert!(other_pack.exists(), "other pack must not be deleted");
}

#[test]
fn download_remove_keeps_files_when_manifest_write_fails() {
    let dir = TempDir::new("remove-manifest-fail");
    let pack_bytes = pack_with("sample", vec![icon("dot", SAMPLE_SVG)]);
    let spec = spec_for("sample", &pack_bytes, LICENCE);
    let installed = install(&pack_bytes, LICENCE, &spec, dir.path()).expect("install");

    let failing_writer = |_path: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "simulated manifest write failure",
        ))
    };
    let err = remove_with("sample", dir.path(), &failing_writer)
        .expect_err("manifest write failure must fail remove");
    assert!(matches!(err, InstallError::Io(_)));
    assert!(
        installed.exists(),
        "pack file must remain when manifest rewrite fails"
    );
}

#[test]
fn download_refuses_total_unpack_over_budget() {
    let pack_bytes = pack_with(
        "sample",
        vec![icon("one", SAMPLE_SVG), icon("two", SAMPLE_SVG)],
    );
    let spec = spec_for("sample", &pack_bytes, LICENCE);
    let total_raw = SAMPLE_SVG.len() * 2;

    validate_pack_with_budget(&pack_bytes, &spec, total_raw)
        .expect("exact unpack budget should pass");
    let err = validate_pack_with_budget(&pack_bytes, &spec, total_raw - 1)
        .expect_err("total decoded bytes over budget must be refused");
    assert_eq!(err, InstallError::Pack(PackError::TooLarge));
}
