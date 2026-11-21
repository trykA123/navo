//! Line-ending settings and the system Git config, read without a program. Feeds the checkout
//! form of a Git side (what `git checkout` would write).

use crate::pointers::refuse_network;
use crate::repo::GitProblem;
use std::ffi::OsString;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// Reads one environment variable; `std::env::var_os` in the app, a map in tests.
pub type EnvVar<'a> = &'a dyn Fn(&str) -> Option<OsString>;
/// Largest system config file read (1 MiB).
pub const MAX_SYSTEM_CONFIG: u64 = 1024 * 1024;

/// A non-empty value of `key`.
fn non_empty(var: EnvVar<'_>, key: &str) -> Option<OsString> {
    var(key).filter(|value| !value.is_empty())
}

/// Where Git would read the system config: `None` when it is switched off or absent. Never
/// starts `git`; `exists` tells whether a candidate file is there.
pub fn system_config_path(
    var: EnvVar<'_>,
    windows: bool,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(flag) = var("GIT_CONFIG_NOSYSTEM") {
        let flag = flag.to_string_lossy().trim().to_ascii_lowercase();
        // Git's bool rules: any integer but 0 is true.
        let number = flag.parse::<i64>().is_ok_and(|n| n != 0);
        if number || matches!(flag.as_str(), "true" | "yes" | "on") {
            return None;
        }
    }
    if let Some(path) = non_empty(var, "GIT_CONFIG_SYSTEM") {
        return Some(PathBuf::from(path));
    }
    if !windows {
        let path = PathBuf::from("/etc/gitconfig");
        return exists(&path).then_some(path);
    }
    let candidates = [
        (
            "ProgramFiles",
            Path::new("Git").join("etc").join("gitconfig"),
        ),
        (
            "ProgramFiles(x86)",
            Path::new("Git").join("etc").join("gitconfig"),
        ),
        (
            "LOCALAPPDATA",
            Path::new("Programs")
                .join("Git")
                .join("etc")
                .join("gitconfig"),
        ),
    ];
    candidates.iter().find_map(|(key, tail)| {
        let path = Path::new(&non_empty(var, key)?).join(tail);
        refuse_network(&path.to_string_lossy()).ok()?;
        exists(&path).then_some(path)
    })
}

/// Reads the system config at `path` once, without following includes. `None` for a network
/// path, an unreadable file, one over `MAX_SYSTEM_CONFIG` bytes, a parse error or a panic.
pub fn read_system(path: &Path) -> Option<gix::config::File> {
    use gix::config::file::Metadata;
    refuse_network(&path.to_string_lossy()).ok()?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_SYSTEM_CONFIG + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_SYSTEM_CONFIG {
        return None;
    }
    let meta = Metadata::from(gix::config::Source::System).at(path);
    catch_unwind(AssertUnwindSafe(|| {
        gix::config::File::from_bytes_no_includes(&bytes, meta, Default::default()).ok()
    }))
    .ok()
    .flatten()
}

/// The system config of this machine, found from the environment and read without includes.
pub fn system_file() -> Option<gix::config::File> {
    system_config_path(&|key| std::env::var_os(key), cfg!(windows), &Path::is_file)
        .and_then(|path| read_system(&path))
}

/// The `core.autocrlf` and `core.eol` a checkout would use: each from the repository, else from
/// `system`. An invalid value, or a parser panic, is `Broken`.
pub fn eol_settings(
    repo: &gix::Repository,
    system: Option<&gix::config::File>,
) -> Result<gix::filter::plumbing::eol::Configuration, GitProblem> {
    use gix::config::tree::Core;
    catch_unwind(AssertUnwindSafe(|| {
        let snapshot = repo.config_snapshot();
        let pick = |key: &str| {
            snapshot
                .string(key)
                .or_else(|| system.and_then(|file| file.string(key)))
        };
        let auto_crlf = match pick("core.autocrlf") {
            Some(value) => Core::AUTO_CRLF
                .try_into_autocrlf(value)
                .map_err(|_| GitProblem::Broken)?,
            None => Default::default(),
        };
        let eol = match pick("core.eol") {
            Some(value) => Some(
                Core::EOL
                    .try_into_eol(value)
                    .map_err(|_| GitProblem::Broken)?,
            ),
            None => None,
        };
        Ok(gix::filter::plumbing::eol::Configuration { auto_crlf, eol })
    }))
    .unwrap_or(Err(GitProblem::Broken))
}

/// `Unsupported` when `core.attributesFile` names a network path. Never touches the file: a
/// stat of `:(optional)` would reach the share.
pub fn check_attributes_file(repo: &gix::Repository) -> Result<(), GitProblem> {
    let found = catch_unwind(AssertUnwindSafe(|| {
        repo.config_snapshot()
            .string("core.attributesFile")
            .map(|value| value.to_string())
    }))
    .map_err(|_| GitProblem::Broken)?;
    let Some(text) = found else { return Ok(()) };
    refuse_network(text.strip_prefix(":(optional)").unwrap_or(&text))
}

#[cfg(test)]
#[path = "checkout_config_tests.rs"]
mod tests;
