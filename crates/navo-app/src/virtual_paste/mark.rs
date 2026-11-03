use std::io;
use std::path::Path;
use std::sync::Arc;

use navo_shell::SourceKind;

/// The Mark of the Web Explorer writes on saved attachments: the Internet zone.
pub(super) const ZONE: &[u8] = b"[ZoneTransfer]\r\nZoneId=3\r\n";

/// A `Zone.Identifier` is a few lines; a bigger one is not trusted.
#[cfg(windows)]
const MAX_ZONE: u64 = 64 * 1024;

/// What goes into each pasted file's `Zone.Identifier` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Mark {
    None,
    Zone(Arc<[u8]>),
}

impl Mark {
    fn internet() -> Self {
        Self::Zone(Arc::from(ZONE))
    }
}

/// Mail and unknown sources may carry files from the internet. A remote session copies files
/// the user already has. Files from a zip folder keep the zip's own mark: the zip's
/// `Zone.Identifier` when it has one, nothing when it has none, the internet mark when the
/// zip is not known or cannot be checked.
pub(super) fn for_source(kind: SourceKind, archive: Option<&Path>) -> Mark {
    decide(kind, archive.map(read_zone))
}

fn decide(kind: SourceKind, archive_zone: Option<io::Result<Vec<u8>>>) -> Mark {
    match kind {
        SourceKind::Mail | SourceKind::Unknown => Mark::internet(),
        SourceKind::RemoteSession => Mark::None,
        SourceKind::Archive => match archive_zone {
            Some(Ok(zone)) => Mark::Zone(zone.into()),
            Some(Err(error)) if error.kind() == io::ErrorKind::NotFound => Mark::None,
            Some(Err(_)) | None => Mark::internet(),
        },
    }
}

/// The zip's `Zone.Identifier`; `NotFound` when the zip has none.
#[cfg(windows)]
fn read_zone(zip: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;

    let mut stream = zip.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let mut zone = Vec::new();
    match std::fs::File::open(stream) {
        Ok(file) => file.take(MAX_ZONE + 1).read_to_end(&mut zone)?,
        // A zip that is gone cannot show that it was unmarked.
        Err(error) if error.kind() == io::ErrorKind::NotFound && !zip.exists() => {
            return Err(io::Error::other("the zip file is gone"));
        }
        Err(error) => return Err(error),
    };
    if zone.len() as u64 > MAX_ZONE {
        return Err(io::Error::other("the zone stream is too large"));
    }
    Ok(zone)
}

/// Other systems have no zone streams.
#[cfg(not(windows))]
fn read_zone(_: &Path) -> io::Result<Vec<u8>> {
    Err(io::ErrorKind::NotFound.into())
}

/// Writes the mark as the file's `Zone.Identifier` stream.
#[cfg(windows)]
pub(super) fn write(path: &Path, zone: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(stream)?;
    file.write_all(zone)?;
    file.sync_all()
}

/// Other systems have no zone streams.
#[cfg(not(windows))]
pub(super) fn write(_: &Path, _: &[u8]) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Option<io::Result<Vec<u8>>> {
        Some(Ok(text.as_bytes().to_vec()))
    }

    fn missing() -> Option<io::Result<Vec<u8>>> {
        Some(Err(io::ErrorKind::NotFound.into()))
    }

    #[test]
    fn mail_and_unknown_sources_get_the_internet_mark_and_remote_sessions_none() {
        for kind in [SourceKind::Mail, SourceKind::Unknown] {
            assert_eq!(decide(kind, None), Mark::internet(), "{kind:?}");
        }
        assert_eq!(decide(SourceKind::RemoteSession, None), Mark::None);
        assert_eq!(ZONE, b"[ZoneTransfer]\r\nZoneId=3\r\n");
    }

    #[test]
    fn an_archive_passes_on_the_zone_of_its_zip() {
        let zone = "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.test/\r\n";
        let expected = Mark::Zone(Arc::from(zone.as_bytes()));
        assert_eq!(decide(SourceKind::Archive, found(zone)), expected);
    }

    #[test]
    fn an_archive_whose_zip_has_no_zone_gets_no_mark() {
        assert_eq!(decide(SourceKind::Archive, missing()), Mark::None);
    }

    #[test]
    fn an_archive_that_cannot_be_checked_fails_closed() {
        let denied = Some(Err(io::ErrorKind::PermissionDenied.into()));
        assert_eq!(decide(SourceKind::Archive, denied), Mark::internet());
        assert_eq!(decide(SourceKind::Archive, None), Mark::internet());
    }
}
