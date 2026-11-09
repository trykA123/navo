//! Folder feeds and local update status for tester builds.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use navo_update::{InstallKind, Version, parse_version};
use serde_json::{Value, json};
use velopack::UpdateManager;
use velopack::sources::{FileSource, NoneSource};

use super::feed::install_kind;

pub(super) fn source() -> io::Result<Option<FileSource>> {
    let result = source_from_dir(std::env::var_os("NAVO_UPDATE_FEED_DIR"));
    if let Err(error) = &result {
        tracing::error!(%error, "update: invalid NAVO_UPDATE_FEED_DIR; updates disabled");
    }
    result
}

fn source_from_dir(directory: Option<OsString>) -> io::Result<Option<FileSource>> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    if directory.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "feed directory must not be empty",
        ));
    }
    let directory = PathBuf::from(directory);
    let metadata = directory.metadata().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("feed directory cannot be opened: {error}"),
        )
    })?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "feed path must be an existing directory",
        ));
    }
    Ok(Some(FileSource::new(directory)))
}

/// Reads local package metadata without checking, downloading or applying an update.
pub(crate) fn status() -> Value {
    match UpdateManager::new(NoneSource {}, None, None) {
        Ok(manager) => {
            let staged = manager
                .get_update_pending_restart()
                .and_then(|asset| parse_version(&asset.Version).ok());
            status_of(
                install_kind(&manager),
                manager.get_current_version(),
                staged,
            )
        }
        Err(error) => {
            tracing::debug!(%error, "update: no package metadata for status");
            json!({
                "kind": "dev",
                "current_version": env!("CARGO_PKG_VERSION"),
                "staged_version": null,
                "state": "unsupported",
            })
        }
    }
}

fn status_of(kind: InstallKind, current: Version, staged: Option<Version>) -> Value {
    let state = match kind {
        InstallKind::Installed if staged.as_ref().is_some_and(|version| version > &current) => {
            "ready"
        }
        InstallKind::Installed => "idle",
        _ => "unsupported",
    };
    let kind = match kind {
        InstallKind::Installed => "installed",
        InstallKind::Portable => "portable",
        InstallKind::Unpackaged => "dev",
    };
    json!({
        "kind": kind,
        "current_version": current.to_string(),
        "staged_version": staged.map(|version| version.to_string()),
        "state": state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use velopack::sources::UpdateSource;

    #[test]
    fn folder_override_reads_and_downloads_from_the_file_source() {
        let directory = tempfile::tempdir().unwrap();
        let feed = json!({"Assets": [{
            "PackageId": "navo", "Version": "1.2.3", "Type": "Full",
            "FileName": "navo.nupkg", "SHA1": "", "SHA256": "", "Size": 7,
            "NotesMarkdown": "", "NotesHtml": ""
        }]});
        std::fs::write(directory.path().join("releases.win.json"), feed.to_string()).unwrap();
        std::fs::write(directory.path().join("navo.nupkg"), b"package").unwrap();
        let source = source_from_dir(Some(directory.path().as_os_str().to_owned()))
            .unwrap()
            .unwrap();
        let feed = source
            .get_release_feed("win", &Default::default(), "")
            .unwrap();
        assert_eq!(feed.Assets.len(), 1);
        assert_eq!(feed.Assets[0].Version, "1.2.3");
        let destination = tempfile::tempdir().unwrap();
        let package = destination.path().join("download.nupkg");
        source
            .download_release_entry(&feed.Assets[0], &package, None)
            .unwrap();
        assert_eq!(std::fs::read(package).unwrap(), b"package");
    }

    #[test]
    fn invalid_override_returns_an_error_instead_of_the_default_source() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("file");
        std::fs::write(&file, b"file").unwrap();
        for path in [PathBuf::new(), directory.path().join("missing"), file] {
            assert!(source_from_dir(Some(path.into_os_string())).is_err());
        }
        assert!(source_from_dir(None).unwrap().is_none());
    }

    #[test]
    fn status_reports_kind_versions_and_local_state() {
        let current = Version::new(1, 0, 0);
        for (kind, text, state) in [
            (InstallKind::Installed, "installed", "idle"),
            (InstallKind::Portable, "portable", "unsupported"),
            (InstallKind::Unpackaged, "dev", "unsupported"),
        ] {
            let status = status_of(kind, current.clone(), None);
            assert_eq!(status["kind"], text);
            assert_eq!(status["state"], state);
            assert!(status["staged_version"].is_null());
            assert_eq!(status["current_version"], "1.0.0");
        }
        let status = status_of(
            InstallKind::Installed,
            current.clone(),
            Some(Version::new(2, 0, 0)),
        );
        assert_eq!(status["staged_version"], "2.0.0");
        assert_eq!(status["state"], "ready");
        let status = status_of(InstallKind::Installed, current.clone(), Some(current));
        assert_eq!(status["state"], "idle");
    }

    #[test]
    fn update_status_is_a_standalone_console_command() {
        use crate::cli::{Command, parse};
        let command = parse([OsString::from("--update-status")]).unwrap();
        assert_eq!(command, Command::UpdateStatus);
        assert!(command.uses_console());
        for args in [
            ["--update-status", "--version"],
            ["--update-status", "folder"],
            ["folder", "--update-status"],
        ] {
            assert!(parse(args.map(OsString::from)).is_err());
        }
    }
}
