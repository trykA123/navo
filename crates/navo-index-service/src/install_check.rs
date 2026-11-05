use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use navo_index::pipe::{SERVICE_REVISION, ServiceVersion};

use super::install_plan::NAVO;

/// The image the service ran before `navo-index.exe`; deleted only past the commit point.
const LEGACY_IMAGE: &str = "navo.exe";

pub(super) fn legacy_command(command: &str) -> bool {
    let command = command.trim_start();
    let image = if let Some(quoted) = command.strip_prefix('"') {
        let Some((image, _)) = quoted.split_once('"') else {
            return false;
        };
        image
    } else {
        command.split_whitespace().next().unwrap_or_default()
    };
    let mut parts = image.rsplit(['\\', '/']);
    parts
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(LEGACY_IMAGE))
        && parts
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("Index"))
}

/// Why a finished install does not run the new image; the elevated run exits with code 3.
#[derive(Debug, thiserror::Error)]
pub(super) enum Mismatch {
    #[error(
        "the service image differs from this Navo (size {found} vs {wanted}, sha256 {found_hash} vs {wanted_hash})"
    )]
    Image {
        found: u64,
        wanted: u64,
        found_hash: String,
        wanted_hash: String,
    },
    #[error("the service is registered as {0}, not the new image")]
    Command(String),
    #[error("the service is not running after the install")]
    NotRunning,
    #[error("the service runs {0}, not the new image")]
    OtherImage(PathBuf),
    #[error("the service process started before the new image was in place")]
    OldProcess,
    #[error("the service process exited before it answered")]
    Exited,
    #[error("the service now runs as process {found}, not the verified {wanted}")]
    Moved { found: u32, wanted: u32 },
    #[error("process {found} answered the pipe, not the verified service process {wanted}")]
    OtherServer { found: u32, wanted: u32 },
    #[error("the service answered {0}, not revision {SERVICE_REVISION}")]
    Revision(String),
}

/// What the SCM reports once the service was started from the new image.
pub(super) struct Running<'a> {
    pub command: &'a OsStr,
    pub process: Option<Process>,
}

pub(super) struct Process {
    pub image: PathBuf,
    /// Creation time in 100 ns ticks (FILETIME).
    pub created: u64,
}

/// The registered command, the image path and the earliest start time a new process can have.
pub(super) struct Expected<'a> {
    pub command: &'a OsStr,
    pub image: &'a Path,
    pub started_after: u64,
}

/// The installed image must have the size and SHA-256 of the source image.
pub(super) fn check_image(source: &Path, image: &Path) -> io::Result<()> {
    let facts = |path: &Path| -> io::Result<(u64, String)> {
        let size = std::fs::metadata(path)?.len();
        let cancel = navo_fs::CancelToken::new();
        let hash = navo_fs::hash::hash_file(path, navo_fs::hash::Algo::Sha256, &cancel, |_| {})
            .map_err(|error| io::Error::other(format!("hashing {}: {error:?}", path.display())))?;
        Ok((size, hash))
    };
    let (wanted, wanted_hash) = facts(source)?;
    let (found, found_hash) = facts(image)?;
    if (found, &found_hash) == (wanted, &wanted_hash) {
        return Ok(());
    }
    Err(io::Error::other(Mismatch::Image {
        found,
        wanted,
        found_hash,
        wanted_hash,
    }))
}

/// The service must be registered with the new command and run a process started after it.
pub(super) fn check_running(expected: &Expected, running: &Running) -> Result<(), Mismatch> {
    if !same_text(running.command, expected.command) {
        return Err(Mismatch::Command(
            running.command.to_string_lossy().into_owned(),
        ));
    }
    let process = running.process.as_ref().ok_or(Mismatch::NotRunning)?;
    if !same_text(process.image.as_os_str(), expected.image.as_os_str()) {
        return Err(Mismatch::OtherImage(process.image.clone()));
    }
    if process.created < expected.started_after {
        return Err(Mismatch::OldProcess);
    }
    Ok(())
}

/// One answer try: the pipe reply and its server id (`None` while silent), whether the retained
/// process still runs, and the process id the SCM reports now.
pub(super) struct Seen<'a> {
    pub reply: Option<(&'a ServiceVersion, u32)>,
    pub alive: bool,
    pub scm: u32,
}

/// `Ok(true)`: the verified `process` answered the current revision; `Ok(false)`: silent, try again.
pub(super) fn check_answer(process: u32, seen: &Seen) -> Result<bool, Mismatch> {
    if !seen.alive {
        return Err(Mismatch::Exited);
    }
    if seen.scm != process {
        return Err(Mismatch::Moved {
            found: seen.scm,
            wanted: process,
        });
    }
    let Some((version, server)) = seen.reply else {
        return Ok(false);
    };
    if server != process {
        return Err(Mismatch::OtherServer {
            found: server,
            wanted: process,
        });
    }
    match version {
        ServiceVersion::Known { revision, .. } if *revision == SERVICE_REVISION => Ok(true),
        other => Err(Mismatch::Revision(format!("{other:?}"))),
    }
}

/// The service image shipped next to the running `navo.exe`; missing, the install refuses.
pub(super) fn service_source(running: &Path) -> io::Result<PathBuf> {
    let source = running.with_file_name(NAVO.image);
    if source.is_file() {
        return Ok(source);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "{} is missing next to {}; reinstall Navo",
            NAVO.image,
            running.display()
        ),
    ))
}

/// The registered command: the quoted image and `run`.
pub(super) fn service_command(image: &Path) -> OsString {
    let mut command = OsString::from("\"");
    command.push(image);
    command.push("\" run");
    command
}

/// Deletes `navo.exe` and its numbered `.old` copies among `paths`; a refused file is scheduled
/// for deletion at restart. The error lists what is left, as a warning.
pub(super) fn delete_legacy(
    paths: impl IntoIterator<Item = PathBuf>,
    mut delete: impl FnMut(&Path) -> io::Result<()>,
    mut schedule: impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut left = Vec::new();
    for path in paths.into_iter().filter(|path| is_legacy(path)) {
        let error = match delete(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => error,
            _ => continue,
        };
        left.push(match schedule(&path) {
            Ok(()) => format!(
                "{} is deleted at the next restart ({error})",
                path.display()
            ),
            Err(refused) => format!(
                "{} was not deleted ({error}) and not scheduled ({refused})",
                path.display()
            ),
        });
    }
    if left.is_empty() {
        return Ok(());
    }
    Err(io::Error::other(left.join("; ")))
}

fn is_legacy(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let Some((head, rest)) = name.split_at_checked(LEGACY_IMAGE.len()) else {
        return false;
    };
    head.eq_ignore_ascii_case(LEGACY_IMAGE)
        && (rest.is_empty()
            || rest
                .strip_prefix('.')
                .and_then(|rest| rest.strip_suffix(".old"))
                .is_some_and(|nonce| {
                    !nonce.is_empty() && nonce.bytes().all(|b| b.is_ascii_digit())
                }))
}

/// Windows paths and service commands compare without case.
fn same_text(left: &OsStr, right: &OsStr) -> bool {
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

pub(super) fn is_mismatch(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<Mismatch>())
}

#[cfg(test)]
#[path = "install_check_tests.rs"]
mod tests;
