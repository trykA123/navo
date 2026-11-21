//! One command-line side: an existing path, else `repo@rev` from the right, else a path.
//! Split from `args.rs` so a later atom can grow the side parser without nearing the file cap.

use std::path::Path;

use sidewise_git_address::{RemoteRev, RemoteUrl};

use crate::args::{ArgError, SideSpec};

/// Longest accepted Git revision, in bytes (NC21).
const MAX_REV_BYTES: usize = 256;

/// Parses one side: an existing whole path, else `repo@rev` from the right, else a path.
/// Fails with `BadRev` when a selected revision breaks NC21.
pub fn side(text: &str, cwd: &Path, exists: &dyn Fn(&Path) -> bool) -> Result<SideSpec, ArgError> {
    let text = crate::quote::unquote(text);
    if let Some((url_text, rev_text)) = split_address(text) {
        let bad = || ArgError::BadUrl(text.to_owned());
        let url = RemoteUrl::parse(url_text).map_err(|_| bad())?;
        let rev = RemoteRev::parse(rev_text).map_err(|_| bad())?;
        return Ok(SideSpec::GitUrl {
            url: url.as_str().to_owned(),
            rev: rev.as_str().to_owned(),
        });
    }
    let whole = cwd.join(text);
    if exists(&whole) {
        return Ok(SideSpec::Path(whole));
    }
    for (at, _) in text.rmatch_indices('@') {
        let repo_text = &text[..at];
        if repo_text.is_empty() {
            continue;
        }
        let repo = cwd.join(repo_text);
        if exists(&repo) {
            let rev = &text[at + 1..];
            check_rev(rev)?;
            return Ok(SideSpec::GitRev {
                repo,
                rev: rev.to_owned(),
            });
        }
    }
    Ok(SideSpec::Path(whole))
}

/// Splits an address attempt (ASCII letters, then `://`) into url text and revision text.
/// The url ends at the first `@` after the first `/` past `://`; no such `@` gives an empty
/// revision. Returns `None` when `text` is not an address attempt.
fn split_address(text: &str) -> Option<(&str, &str)> {
    let letters = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters == 0 || !text[letters..].starts_with("://") {
        return None;
    }
    let after = letters + 3;
    let Some(slash) = text[after..].find('/').map(|i| after + i) else {
        return Some((text, ""));
    };
    match text[slash..].find('@') {
        Some(i) => Some((&text[..slash + i], &text[slash + i + 1..])),
        None => Some((text, "")),
    }
}

/// Accepts a revision unless it is empty, dash-led, control-bearing or over 256 bytes.
fn check_rev(rev: &str) -> Result<(), ArgError> {
    let bad = rev.is_empty()
        || rev.starts_with('-')
        || rev.chars().any(char::is_control)
        || rev.len() > MAX_REV_BYTES;
    if bad {
        return Err(ArgError::BadRev(rev.to_owned()));
    }
    Ok(())
}
