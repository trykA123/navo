//! Linux and Windows tests for payload metadata, integrity and log retention.

use super::*;
use std::io::Write;

/// Writes a tiny full-package fixture containing the supplied nuspec documents.
pub(super) fn fixture(
    root: &Path,
    documents: &[&str],
) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let setup = root.join("input.exe");
    fs::write(&setup, b"fixture Setup.exe")?;
    let package = root.join("NavoApp-1.2.3-full.nupkg");
    let mut zip = zip::ZipWriter::new(fs::File::create(&package)?);
    for (index, document) in documents.iter().enumerate() {
        zip.start_file(
            format!("package{index}.nuspec"),
            zip::write::SimpleFileOptions::default(),
        )?;
        zip.write_all(document.as_bytes())?;
    }
    zip.finish()?;
    Ok((setup, package))
}

/// Builds the same package metadata shape used by sq.version and NuGet.
pub(super) fn nuspec(id: &str, version: &str) -> String {
    format!(
        "<package xmlns='urn:nuget'><metadata><id>{id}</id><version>{version}</version></metadata></package>"
    )
}

/// Provides a tiny validated payload for platform extraction tests.
pub(super) fn file_payload(root: &Path) -> Result<Payload, Box<dyn std::error::Error>> {
    let (setup, package) = fixture(root, &[&nuspec("NavoApp", "1.2.3")])?;
    Ok(Payload::from_files(
        &setup,
        &package,
        parse_version("1.2.3")?,
    )?)
}

/// Embedded releases use the crate version; ordinary CI builds report missing payloads.
#[test]
fn embedded_metadata_matches_build_configuration() -> Result<(), Box<dyn std::error::Error>> {
    if embedded::AVAILABLE {
        let payload = Payload::embedded()?;
        assert_eq!(payload.version, parse_version(env!("CARGO_PKG_VERSION"))?);
        assert_eq!(payload.setup_size, embedded::SETUP.len() as u64);
        assert_eq!(payload.package_size, embedded::PACKAGE.len() as u64);
        assert_eq!(payload.package_name, embedded::PACKAGE_NAME);
    } else {
        let error = Payload::embedded()
            .err()
            .ok_or("empty build unexpectedly has payload")?;
        assert!(matches!(error, PayloadError::Missing));
        assert_eq!(error.exit_code(), 9);
    }
    Ok(())
}

/// Tester metadata uses its supplied version, file name, sizes and SHA-256 values.
#[test]
fn file_payload_retains_supplied_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let payload = file_payload(root.path())?;
    assert_eq!(payload.version, parse_version("1.2.3")?);
    assert_eq!(payload.package_name, "NavoApp-1.2.3-full.nupkg");
    assert_eq!(payload.setup_size, 17);
    assert_eq!(
        payload.setup_sha256,
        <[u8; 32]>::from(Sha256::digest(b"fixture Setup.exe"))
    );
    assert_eq!(
        payload.package_sha256,
        <[u8; 32]>::from(Sha256::digest(payload.package()))
    );
    assert_eq!(payload.package_size, payload.package().len() as u64);
    Ok(())
}

/// A foreign package id or a different version is rejected with exit 9.
#[test]
fn wrong_nuspec_identity_refuses_with_exit_9() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    for (id, version) in [
        ("Foreign", "1.2.3"),
        ("NavoApp", "1.2.4"),
        ("NavoApp", "1.2.3-beta"),
    ] {
        let (setup, package) = fixture(root.path(), &[&nuspec(id, version)])?;
        let result = Payload::from_files(&setup, &package, parse_version("1.2.3")?);
        let error = result.err().ok_or("foreign package accepted")?;
        assert_eq!(error.exit_code(), 9);
        assert!(matches!(error, PayloadError::Damaged));
    }
    Ok(())
}

/// Delta names and names with a different version are damaged payloads with exit 9.
#[test]
fn wrong_package_name_refuses_with_exit_9() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    for name in ["NavoApp-1.2.3-delta.nupkg", "NavoApp-1.2.4-full.nupkg"] {
        let (setup, package) = fixture(root.path(), &[&nuspec("NavoApp", "1.2.3")])?;
        let renamed = root.path().join(name);
        fs::rename(package, &renamed)?;
        let error = Payload::from_files(&setup, &renamed, parse_version("1.2.3")?)
            .err()
            .ok_or("wrong package name accepted")?;
        assert!(matches!(error, PayloadError::Damaged));
        assert_eq!(error.exit_code(), 9);
    }
    Ok(())
}

/// Missing, duplicate, malformed and oversized nuspec documents all fail closed.
#[test]
fn damaged_nuspec_refuses_with_exit_9() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let valid = nuspec("NavoApp", "1.2.3");
    let oversized = "x".repeat(64 * 1024 + 1);
    for documents in [
        vec![],
        vec![valid.as_str(), valid.as_str()],
        vec!["<broken"],
        vec![oversized.as_str()],
    ] {
        let (setup, package) = fixture(root.path(), &documents)?;
        let error = Payload::from_files(&setup, &package, parse_version("1.2.3")?)
            .err()
            .ok_or("damaged package accepted")?;
        assert_eq!(error.exit_code(), 9);
    }
    Ok(())
}

/// SHA-256 matches only unchanged bytes of exactly the expected size.
#[test]
fn hash_check_refuses_corruption_and_wrong_sizes() -> io::Result<()> {
    let bytes = b"abc";
    let hash: [u8; 32] = Sha256::digest(bytes).into();
    verify_hash(&mut bytes.as_slice(), &hash, 3)?;
    for size in [2, 4] {
        assert_eq!(
            verify_hash(&mut bytes.as_slice(), &hash, size)
                .err()
                .map(|e| e.kind()),
            Some(io::ErrorKind::InvalidData)
        );
    }
    assert!(verify_hash(&mut b"bad".as_slice(), &hash, 3).is_err());
    Ok(())
}

/// Rotation keeps five whole runs, their companions, reparse points and unrelated files.
#[test]
fn rotation_retains_five_runs_and_skips_reparse_points() {
    let mut entries = Vec::new();
    for day in 1..=7 {
        for suffix in [".log", "-velopack.log", "-velopack.log.old", ".log.old"] {
            entries.push(LogEntry {
                path: format!("202611{day:02}T120000000Z{suffix}").into(),
                reparse: false,
            });
        }
    }
    let link = PathBuf::from("20261101T110000000Z.log");
    entries.push(LogEntry {
        path: link.clone(),
        reparse: true,
    });
    entries.push(LogEntry {
        path: "unrelated.log".into(),
        reparse: false,
    });
    let removed = logs_to_remove(&entries);
    assert_eq!(removed.len(), 8);
    assert!(
        removed
            .iter()
            .all(|path| path.to_string_lossy().starts_with("20261101")
                || path.to_string_lossy().starts_with("20261102"))
    );
    assert!(!removed.contains(&link));
    assert!(logs_to_remove(&entries[8..]).is_empty());
}
