use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use gix_config::file::SectionRef;
use gix_config::file::section::HeaderRef;
use gix_config::{File, Source};

use crate::GitError;
use crate::format::common_dir;
use crate::status::Reason;

mod listed;
mod load;

/// `-c` pairs that stop Git from starting gc or maintenance after the command.
#[must_use]
pub fn maintenance_off() -> Vec<OsString> {
    MAINTENANCE_PAIRS
        .iter()
        .flat_map(|pair| ["-c", pair])
        .map(OsString::from)
        .collect()
}

/// The key reported when a config file or include could not be read or parsed.
pub const UNREADABLE_KEY: &str = "config-unreadable";

/// The `git fetch` flag a background fetch must pass, so no maintenance hook runs after it.
pub const NO_AUTO_MAINTENANCE: &str = "--no-auto-maintenance";

const STATUS_PAIRS: [&str; 2] = ["core.fsmonitor=false", "core.useBuiltinFSMonitor=false"];

const MAINTENANCE_PAIRS: [&str; 2] = ["gc.auto=0", "maintenance.auto=false"];

const NEUTRAL_FILTER: [(&str, &str); 4] = [
    ("clean", ""),
    ("smudge", ""),
    ("process", ""),
    ("required", "false"),
];

/// What a repository's own config could run, read without running Git (D100e).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guard {
    filters: BTreeSet<OsString>,
    keys: BTreeMap<String, Values>,
    blocked: bool,
    special_file: bool,
}

impl Guard {
    /// Reads the repository-local config of `git_dir`; `~/` includes resolve against `HOME`.
    pub fn read(git_dir: &Path) -> Result<Self, GitError> {
        Self::read_with_home(git_dir, home_dir().as_deref())
    }

    /// Reads `config`, `config.worktree` and every include of them; never global or system files.
    pub fn read_with_home(git_dir: &Path, home: Option<&Path>) -> Result<Self, GitError> {
        let common = match common_dir(git_dir) {
            Ok(None) => git_dir.to_path_buf(),
            Ok(Some(common)) if common.is_dir() => common,
            _ => return Ok(Self::unreadable()),
        };
        let roots = [
            (common.join("config"), Source::Local),
            (git_dir.join("config.worktree"), Source::Worktree),
        ];
        let loaded = load::load(&roots, home);
        let mut guard = if loaded.unreadable {
            Self::unreadable()
        } else {
            Self::default()
        };
        guard.special_file = loaded.special;
        for file in &loaded.files {
            guard.scan(file);
        }
        Ok(guard)
    }

    pub(crate) fn unreadable() -> Self {
        Self {
            blocked: true,
            keys: BTreeMap::from([(UNREADABLE_KEY.to_owned(), Values::default())]),
            ..Self::default()
        }
    }

    /// `-c` pairs for every background call: fsmonitor off and every filter found neutralised.
    #[must_use]
    pub fn args(&self) -> Vec<OsString> {
        let mut args = Vec::new();
        for pair in STATUS_PAIRS {
            args.push(OsString::from("-c"));
            args.push(OsString::from(pair));
        }
        for name in &self.filters {
            for (key, value) in NEUTRAL_FILTER {
                let mut pair = OsString::from("filter.");
                pair.push(name);
                pair.push(format!(".{key}={value}"));
                args.push(OsString::from("-c"));
                args.push(pair);
            }
        }
        args
    }

    /// `args` plus no automatic gc or maintenance and no hook files, for a background fetch.
    /// The fetch must also pass [`NO_AUTO_MAINTENANCE`] and never run when `is_guarded`.
    #[must_use]
    pub fn fetch_args(&self) -> Vec<OsString> {
        let mut args = self.args();
        args.extend(maintenance_off());
        args.extend(["-c", "core.hooksPath="].map(OsString::from));
        args
    }

    /// True when the local config sets a guard-list key; the background fetch skips it.
    #[must_use]
    pub fn is_guarded(&self) -> bool {
        !self.keys.is_empty()
    }

    /// True when `args` cannot cover the config, so no background Git call may run.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }

    /// The guard-list keys found, as `section[.subsection].key`, sorted.
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        self.keys.keys().map(String::as_str).collect()
    }

    fn scan(&mut self, file: &File) {
        for section in file.sections() {
            let flat = flat_name(&section.header());
            let (name, subsection) = match flat.iter().position(|byte| *byte == b'.') {
                Some(dot) => (&flat[..dot], Some(&flat[dot + 1..])),
                None => (&flat[..], None),
            };
            self.note_section((name, subsection), &section);
            if name == b"filter"
                && let Some(sub) = subsection
            {
                self.add_filter(sub);
            }
        }
    }

    fn note_section(
        &mut self,
        (name, subsection): (&[u8], Option<&[u8]>),
        section: &SectionRef<'_>,
    ) {
        let keys: BTreeSet<String> = section.value_names().collect();
        for key in keys {
            let Some(held) = self.note(name, subsection, &key.to_ascii_lowercase()) else {
                continue;
            };
            held.extend(section.values(&key).into_iter().map(Vec::from));
        }
    }

    /// Records a guard-list key and returns its values to extend; `None` for any other key.
    fn note(
        &mut self,
        section: &[u8],
        subsection: Option<&[u8]>,
        key: &str,
    ) -> Option<&mut Vec<Vec<u8>>> {
        is_guard_key(section, key).then(|| {
            let name = key_name(section, subsection, key);
            &mut self.keys.entry(name).or_default().0
        })
    }

    fn add_filter(&mut self, name: &[u8]) {
        let expressible = !name.iter().any(|byte| matches!(byte, b'=' | b'\n' | 0));
        match os_from_bytes(name) {
            Some(name) if expressible => {
                self.filters.insert(name);
            }
            _ => self.blocked = true,
        }
    }
}

/// The values of one guard-list key in read order, compared so a replaced value counts as a
/// change; `Debug` shows only their number, since a value may hold a credential.
#[derive(Clone, Default, PartialEq, Eq)]
struct Values(Vec<Vec<u8>>);

impl fmt::Debug for Values {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} value(s)", self.0.len())
    }
}

/// Why a status read refused to run, as the badge and its text tell it apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Git does not trust the folder's owner (`safe.directory`).
    Ownership,
    /// A filter driver other than the standard git-lfs one is in use.
    Filter,
    /// The config names a program such as fsmonitor.
    Program,
    /// A config file could not be read safely, so the guard blocked every call.
    Guard,
}

/// The refusal behind `error`, when it is one.
#[must_use]
pub fn refusal(error: &GitError) -> Option<Refusal> {
    match error {
        GitError::UnsafeDirectory { .. } => Some(Refusal::Ownership),
        GitError::FilterRefused { .. } => Some(Refusal::Filter),
        GitError::ProgramConfigured { .. } => Some(Refusal::Program),
        GitError::NotTrusted { .. } => Some(Refusal::Guard),
        GitError::Command { stderr, .. }
            if stderr.contains("detected dubious ownership")
                || stderr.contains("unsafe repository") =>
        {
            Some(Refusal::Ownership)
        }
        _ => None,
    }
}

/// `Unknown(NotTrusted)` when Git refused the repository as an unsafe directory, or the guard
/// blocked every background call, or the built-in engine refused a filter or program.
#[must_use]
pub fn untrusted_reason(error: &GitError) -> Option<Reason> {
    refusal(error).map(|_| Reason::NotTrusted)
}

/// The `section.subsection` prefix Git flattens a header to, so `[filter.a "b"]` is `filter.a.b`.
fn flat_name(header: &HeaderRef<'_>) -> Vec<u8> {
    let mut flat = header.name().to_ascii_lowercase();
    if let Some(sub) = header.subsection_name() {
        flat.push(b'.');
        if header.is_legacy() {
            flat.extend(sub.to_ascii_lowercase());
        } else {
            flat.extend_from_slice(sub);
        }
    }
    flat
}

fn is_guard_key(section: &[u8], key: &str) -> bool {
    matches!(
        (section, key),
        (b"filter", _)
            | (
                b"core",
                "fsmonitor"
                    | "usebuiltinfsmonitor"
                    | "hookspath"
                    | "sshcommand"
                    | "gitproxy"
                    | "askpass"
                    | "alternaterefscommand"
            )
            | (b"credential", "helper")
            | (b"remote", "uploadpack" | "receivepack" | "vcs")
            | (b"diff", "external" | "textconv" | "command")
            | (b"gpg", "program")
            | (b"uploadpack", "packobjectshook")
            | (b"hook", _)
            | (b"gc", "recentobjectshook")
            | (b"protocol", "allow")
    )
}

fn key_name(section: &[u8], subsection: Option<&[u8]>, key: &str) -> String {
    let mut name = String::from_utf8_lossy(section).into_owned();
    if let Some(sub) = subsection {
        name.push('.');
        name.extend(
            String::from_utf8_lossy(sub)
                .chars()
                .map(|c| if c.is_control() { '?' } else { c }),
        );
    }
    name.push('.');
    name.push_str(key);
    name
}

fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|home| !home.is_empty())
        .map(PathBuf::from)
}

#[cfg(unix)]
fn os_from_bytes(bytes: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;

    Some(OsString::from_vec(bytes.to_vec()))
}

#[cfg(windows)]
fn os_from_bytes(bytes: &[u8]) -> Option<OsString> {
    String::from_utf8(bytes.to_vec()).ok().map(OsString::from)
}
