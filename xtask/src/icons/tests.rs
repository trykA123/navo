use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use super::*;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a unique temporary directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "navo-xtask-icons-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Builds an in-memory gzipped tarball from `(path, bytes)` entries.
fn build_tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, *bytes).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

/// Computes lowercase hex SHA-256 for test fixtures.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Workspace root derived from `xtask`'s `CARGO_MANIFEST_DIR`.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn icons_refuses_sha_mismatch() {
    let temp = TempDir::new("sha-mismatch");
    let tarballs = temp.path().join("tarballs");
    std::fs::create_dir_all(&tarballs).unwrap();

    let bad_bytes = b"not the expected tarball";
    let actual = sha256_hex(bad_bytes);
    let expected = "0".repeat(64);
    std::fs::write(tarballs.join("alpha-1.0.0.tgz"), bad_bytes).unwrap();

    let config_path = temp.path().join("sources.toml");
    let config = format!(
        r#"
download = []

[[bundled]]
package = "alpha"
version = "1.0.0"
url = "https://example.invalid/alpha/-/alpha-1.0.0.tgz"
sha256 = "{expected}"
licence = "MIT"

[[bundled.sets]]
id = "alpha"
grid = 24
kind = "nodes"
file = "package/nodes.json"
style = "stroke"
"#
    );
    std::fs::write(&config_path, config).unwrap();

    let args: Vec<String> = [
        "pack",
        "--tarballs",
        tarballs.to_str().unwrap(),
        "--config",
        config_path.to_str().unwrap(),
        "--out",
        temp.path().join("bundled.nvic").to_str().unwrap(),
        "--licences",
        temp.path().join("licences").to_str().unwrap(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    let err = execute(&args).unwrap_err();
    assert!(
        err.contains(&expected),
        "error should include expected sha256 {expected}, got: {err}"
    );
    assert!(
        err.contains(&actual),
        "error should include actual sha256 {actual}, got: {err}"
    );
}

#[test]
fn icons_sources_toml_parses() {
    let path = workspace_root().join("assets/icons/sources.toml");
    let config = load_config(&path).unwrap();

    assert_eq!(config.bundled.len(), 4);
    let set_ids: Vec<&str> = config
        .bundled
        .iter()
        .flat_map(|pkg| pkg.sets.iter().map(|set| set.id.as_str()))
        .collect();
    assert_eq!(
        set_ids,
        [
            "lucide",
            "tabler",
            "tabler-filled",
            "phosphor",
            "phosphor-fill",
            "brands",
        ]
    );
    for pkg in &config.bundled {
        assert_eq!(
            pkg.sha256.len(),
            64,
            "sha256 for {} must be 64 hex chars",
            pkg.package
        );
        assert!(
            pkg.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "sha256 for {} must be hex",
            pkg.package
        );
    }
    let dl_ids: Vec<&str> = config.download.iter().map(|dl| dl.id.as_str()).collect();
    assert_eq!(dl_ids, ["fluent", "fluent-filled", "material", "carbon"]);
}

/// Writes two tiny fixture tarballs and a matching `sources.toml` under `root`.
fn write_two_set_fixtures(root: &Path) -> (PathBuf, PathBuf) {
    let tarballs = root.join("tarballs");
    std::fs::create_dir_all(&tarballs).unwrap();

    let nodes_json = br#"{"arrow":[["path",{"d":"M5 12h14"}]],"dot":[["circle",{"cx":"12","cy":"12","r":"4"}]]}"#;
    let alpha_tgz = build_tgz(&[
        ("package/icon-nodes.json", nodes_json),
        ("package/LICENSE", b"MIT License Alpha\n"),
    ]);
    let alpha_sha = sha256_hex(&alpha_tgz);
    std::fs::write(tarballs.join("alpha-1.0.0.tgz"), &alpha_tgz).unwrap();

    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2L20 20" fill="none" stroke="#000" stroke-width="2"/></svg>"##;
    let beta_tgz = build_tgz(&[
        ("package/svg/star.svg", svg),
        ("package/LICENSE.md", b"ISC License Beta\n"),
    ]);
    let beta_sha = sha256_hex(&beta_tgz);
    std::fs::write(tarballs.join("beta-2.0.0.tgz"), &beta_tgz).unwrap();

    let config_path = root.join("sources.toml");
    let config = format!(
        r#"
download = []

[[bundled]]
package = "alpha"
version = "1.0.0"
url = "https://example.invalid/alpha/-/alpha-1.0.0.tgz"
sha256 = "{alpha_sha}"
licence = "MIT"

[[bundled.licences]]
from = "package/LICENSE"
to = "alpha-LICENSE"

[[bundled.sets]]
id = "alpha"
grid = 24
kind = "nodes"
file = "package/icon-nodes.json"
style = "stroke"

[[bundled]]
package = "beta"
version = "2.0.0"
url = "https://example.invalid/beta/-/beta-2.0.0.tgz"
sha256 = "{beta_sha}"
licence = "ISC"

[[bundled.licences]]
from = "package/LICENSE.md"
to = "beta-LICENSE.md"

[[bundled.sets]]
id = "beta"
grid = 24
kind = "svg-dir"
dir = "package/svg"
suffix = ".svg"
strip = ""
"#
    );
    std::fs::write(&config_path, config).unwrap();
    (tarballs, config_path)
}

#[test]
fn icons_pack_prints_one_line_per_set() {
    let temp = TempDir::new("pack-and-verify");
    let (tarballs, config_path) = write_two_set_fixtures(temp.path());
    let licences = temp.path().join("licences");
    let out_pack = temp.path().join("bundled.nvic");

    let pack_args: Vec<String> = [
        "pack",
        "--tarballs",
        tarballs.to_str().unwrap(),
        "--config",
        config_path.to_str().unwrap(),
        "--out",
        out_pack.to_str().unwrap(),
        "--licences",
        licences.to_str().unwrap(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    let lines = execute(&pack_args).unwrap();
    assert_eq!(lines.len(), 2, "expected one line per set, got: {lines:?}");
    assert!(lines[0].starts_with("alpha: 2 icons ("), "{}", lines[0]);
    assert!(lines[0].ends_with(" bytes)"), "{}", lines[0]);
    assert!(lines[1].starts_with("beta: 1 icons ("), "{}", lines[1]);
    assert!(lines[1].ends_with(" bytes)"), "{}", lines[1]);

    assert!(out_pack.is_file(), "bundled.nvic should be written");
    assert_eq!(
        std::fs::read(licences.join("alpha-LICENSE")).unwrap(),
        b"MIT License Alpha\n"
    );
    assert_eq!(
        std::fs::read(licences.join("beta-LICENSE.md")).unwrap(),
        b"ISC License Beta\n"
    );

    let verify_args: Vec<String> = [
        "verify",
        "--tarballs",
        tarballs.to_str().unwrap(),
        "--config",
        config_path.to_str().unwrap(),
        "--pack",
        out_pack.to_str().unwrap(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    let verify_lines = execute(&verify_args).unwrap();
    assert_eq!(verify_lines, ["bundled.nvic matches"]);
}

/// Writes a `sources.toml` fixture for the `gamma` bundled + download test.
fn write_gamma_config(
    path: &Path,
    tgz: (&str, usize),
    pack: (&str, usize),
    licence: (&str, usize),
) {
    let config = format!(
        r#"
[[bundled]]
package = "gamma"
version = "1.0.0"
url = "https://example.invalid/gamma/-/gamma-1.0.0.tgz"
sha256 = "{tgz_sha}"
licence = "MIT"

[[bundled.sets]]
id = "gamma"
grid = 24
kind = "svg-dir"
dir = "package/svg"
suffix = ".svg"
strip = ""

[[download]]
id = "gamma"
package = "gamma"
version = "1.0.0"
url = "https://example.invalid/gamma/-/gamma-1.0.0.tgz"
sha256 = "{tgz_sha}"
size = {tgz_len}
pack_path = "/trykA123/navo/releases/download/icon-sets-1/gamma.nvic"
pack_size = {pack_len}
pack_sha256 = "{pack_sha}"
licence = "MIT"
licence_file = "package/LICENSE"
licence_path = "/trykA123/navo/releases/download/icon-sets-1/gamma-LICENSE"
licence_size = {lic_len}
licence_sha256 = "{lic_sha}"
grid = 24
kind = "svg-dir"
dir = "package/svg"
suffix = ".svg"
strip = ""
"#,
        tgz_sha = tgz.0,
        tgz_len = tgz.1,
        pack_sha = pack.0,
        pack_len = pack.1,
        lic_sha = licence.0,
        lic_len = licence.1,
    );
    std::fs::write(path, config).unwrap();
}

#[test]
fn icons_verify_reports_mismatch_and_downloads() {
    let temp = TempDir::new("verify-extra");
    let tarballs = temp.path().join("tarballs");
    let out_pack = temp.path().join("bundled.nvic");
    let dl_out = temp.path().join("packs");
    std::fs::create_dir_all(&tarballs).unwrap();

    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2L20 20" fill="none" stroke="#000" stroke-width="2"/></svg>"##;
    let licence_bytes = b"MIT License Gamma\n";
    let tgz = build_tgz(&[
        ("package/svg/box.svg", svg),
        ("package/LICENSE", licence_bytes),
    ]);
    let sha = sha256_hex(&tgz);
    let lic_sha = sha256_hex(licence_bytes);
    let zero_sha = "0".repeat(64);
    std::fs::write(tarballs.join("gamma-1.0.0.tgz"), &tgz).unwrap();

    let config_path = temp.path().join("sources.toml");
    write_gamma_config(
        &config_path,
        (&sha, tgz.len()),
        (&zero_sha, 0),
        (&lic_sha, licence_bytes.len()),
    );
    std::fs::write(&out_pack, b"corrupted pack bytes").unwrap();

    let verify_args = [
        "verify",
        "--tarballs",
        tarballs.to_str().unwrap(),
        "--config",
        config_path.to_str().unwrap(),
        "--pack",
        out_pack.to_str().unwrap(),
    ]
    .map(str::to_owned);
    let err = execute(&verify_args).unwrap_err();
    assert!(err.contains("mismatch"), "{err}");
    assert!(err.contains("offset 0"), "{err}");

    let pack_dl_args = [
        "pack",
        "--downloads",
        "--tarballs",
        tarballs.to_str().unwrap(),
        "--config",
        config_path.to_str().unwrap(),
        "--out",
        dl_out.to_str().unwrap(),
    ]
    .map(str::to_owned);
    assert_eq!(execute(&pack_dl_args).unwrap().len(), 1);
    let built_pack = std::fs::read(dl_out.join("gamma.nvic")).unwrap();
    let built_sha = sha256_hex(&built_pack);
    assert_eq!(
        std::fs::read(dl_out.join("gamma-LICENSE")).unwrap(),
        licence_bytes
    );

    write_gamma_config(
        &config_path,
        (&sha, tgz.len()),
        (&built_sha, built_pack.len()),
        (&lic_sha, licence_bytes.len()),
    );
    let dl_args = [
        "verify",
        "--downloads",
        "--tarballs",
        tarballs.to_str().unwrap(),
        "--config",
        config_path.to_str().unwrap(),
    ]
    .map(str::to_owned);
    assert_eq!(
        execute(&dl_args).unwrap(),
        [format!(
            "gamma matches pack={built_sha} ({} bytes) licence={lic_sha} ({} bytes)",
            built_pack.len(),
            licence_bytes.len()
        )]
    );
}

#[test]
fn icons_refuses_unsafe_licence_destination() {
    let temp = TempDir::new("unsafe-licence");
    let tarballs = temp.path().join("tarballs");
    let licences = temp.path().join("licences");
    let escape_rel = temp.path().join("escape.txt");
    let escape_abs = temp.path().join("abs-escape.txt");
    std::fs::create_dir_all(&tarballs).unwrap();

    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2L20 20" fill="none" stroke="#000" stroke-width="2"/></svg>"##;
    let tgz = build_tgz(&[("package/svg/box.svg", svg), ("package/LICENSE", b"MIT\n")]);
    let sha = sha256_hex(&tgz);
    std::fs::write(tarballs.join("delta-1.0.0.tgz"), &tgz).unwrap();

    for bad_to in [
        "../escape.txt".to_owned(),
        escape_abs.to_str().unwrap().to_owned(),
        "sub/dir.txt".to_owned(),
        String::new(),
    ] {
        let config_path = temp.path().join("sources.toml");
        let config = format!(
            r#"
download = []

[[bundled]]
package = "delta"
version = "1.0.0"
url = "https://example.invalid/delta/-/delta-1.0.0.tgz"
sha256 = "{sha}"
licence = "MIT"

[[bundled.licences]]
from = "package/LICENSE"
to = "{bad_to}"

[[bundled.sets]]
id = "delta"
grid = 24
kind = "svg-dir"
dir = "package/svg"
suffix = ".svg"
strip = ""
"#
        );
        std::fs::write(&config_path, config).unwrap();
        let pack_args: Vec<String> = [
            "pack",
            "--tarballs",
            tarballs.to_str().unwrap(),
            "--config",
            config_path.to_str().unwrap(),
            "--out",
            temp.path().join("bundled.nvic").to_str().unwrap(),
            "--licences",
            licences.to_str().unwrap(),
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        let err = execute(&pack_args).expect_err("unsafe licence `to` path must fail");
        assert!(
            err.contains("licence"),
            "error should mention licence filename, got: {err}"
        );
    }

    assert!(!escape_rel.exists(), "must not write ../escape.txt");
    assert!(!escape_abs.exists(), "must not write absolute path");
}
