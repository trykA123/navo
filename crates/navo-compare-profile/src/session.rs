//! Session records with resolved sides and engine rule values (NS9), plus the
//! format version and id rules the store enforces.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sidewise_git_address::{RemoteRev, RemoteUrl};

/// The layout version written to each session file.
pub const SESSION_VERSION: u32 = 1;

/// How many unnamed sessions the recent list keeps (NC8).
pub const RECENT_KEPT: usize = 20;

/// The whitespace values the compare engine understands (NS9).
pub(crate) const WHITESPACE_VALUES: [&str; 4] = ["keep", "all", "changes", "trailing"];

/// The filter values a session may store.
pub(crate) const FILTER_VALUES: [&str; 4] = ["all", "differences", "orphans", "same"];

/// One saved compare: two resolved sides, rules, filter and view stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Format version of the file.
    pub version: u32,
    /// File name stem, valid per NS5.
    pub id: String,
    /// User-given name; `None` makes it a recent session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the session compares folders or two texts.
    pub kind: SessionKind,
    /// Left side.
    pub left: SideRef,
    /// Right side.
    pub right: SideRef,
    /// Compare rules.
    pub rules: Rules,
    /// Active filter: `all`, `differences`, `orphans` or `same`.
    pub filter: String,
    /// When the session was opened, in Unix seconds.
    pub opened: i64,
    /// Opaque view stack of NC8.
    #[serde(default)]
    pub view: Vec<String>,
}

/// What a session compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    /// Two folders or two Git revisions.
    Pair,
    /// Two pasted texts, which are never stored (NC26).
    Texts,
}

/// One resolved side: an absolute path, a Git revision, or a remote address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideRef {
    /// Whether the side is a path, a Git revision or a remote repository.
    pub kind: SideKind,
    /// Absolute path (NS9); empty for a remote side.
    pub path: PathBuf,
    /// Git revision; `None` for a path side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// Canonical https address of a remote side; `None` for path and Git sides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl SideRef {
    /// Makes a side that reads `path` from disk.
    pub fn path(path: PathBuf) -> SideRef {
        SideRef {
            kind: SideKind::Path,
            path,
            rev: None,
            url: None,
        }
    }

    /// Makes a side that reads revision `rev` of the Git repository at `repo`.
    pub fn git(repo: PathBuf, rev: String) -> SideRef {
        SideRef {
            kind: SideKind::Git,
            path: repo,
            rev: Some(rev),
            url: None,
        }
    }

    /// Makes a remote side: canonical address `url` and revision `rev`; the path stays empty.
    pub fn remote(url: String, rev: String) -> SideRef {
        SideRef {
            kind: SideKind::Remote,
            path: PathBuf::new(),
            rev: Some(rev),
            url: Some(url),
        }
    }
}

/// Where a side's content comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SideKind {
    /// A file, folder or archive on disk.
    Path,
    /// A revision read through Git.
    Git,
    /// A revision of a remote https Git repository, read from Sidewise's own cache.
    Remote,
}

/// The compare rules stored with a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    /// Whitespace handling: `keep`, `all`, `changes` or `trailing`.
    pub whitespace: String,
    /// Ignore a byte-order mark.
    pub ignore_bom: bool,
    /// Treat different line endings as equal.
    pub ignore_line_endings: bool,
    /// Compare case-insensitively.
    pub ignore_case: bool,
    /// Leave out files ignored by `.gitignore`.
    pub respect_gitignore: bool,
}

/// Returns true when `id` is 1–40 characters of `0-9`, `a-f` or `-` (NS5).
pub(crate) fn is_valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) || byte == b'-')
}

/// Returns true when the whitespace value is one of the engine's four names.
pub(crate) fn rules_valid(rules: &Rules) -> bool {
    WHITESPACE_VALUES.contains(&rules.whitespace.as_str())
}

/// Returns true when the filter is one of the four stored names.
pub(crate) fn filter_valid(filter: &str) -> bool {
    FILTER_VALUES.contains(&filter)
}

/// Returns true when both sides are absolute and their rev matches their kind.
pub(crate) fn sides_valid(session: &Session) -> bool {
    side_valid(&session.left) && side_valid(&session.right)
}

/// Returns true when one side can be stored under NS9.
fn side_valid(side: &SideRef) -> bool {
    match side.kind {
        SideKind::Path => side.path.is_absolute() && side.rev.is_none() && side.url.is_none(),
        SideKind::Git => {
            side.path.is_absolute()
                && side.url.is_none()
                && side.rev.as_deref().is_some_and(rev_valid)
        }
        SideKind::Remote => {
            side.path.as_os_str().is_empty()
                && remote_valid(side.url.as_deref(), side.rev.as_deref())
        }
    }
}

/// Returns true when a Git revision can be stored: 1–256 bytes, not starting
/// with `-`, and without control characters.
fn rev_valid(rev: &str) -> bool {
    !rev.is_empty()
        && rev.len() <= 256
        && !rev.starts_with('-')
        && !rev.chars().any(char::is_control)
}

/// Returns true when `url` and `rev` are canonical remote values; false otherwise.
fn remote_valid(url: Option<&str>, rev: Option<&str>) -> bool {
    let (Some(url), Some(rev)) = (url, rev) else {
        return false;
    };
    let Ok(parsed_url) = RemoteUrl::parse(url) else {
        return false;
    };
    let Ok(parsed_rev) = RemoteRev::parse(rev) else {
        return false;
    };
    parsed_url.as_str() == url && parsed_rev.as_str() == rev
}
