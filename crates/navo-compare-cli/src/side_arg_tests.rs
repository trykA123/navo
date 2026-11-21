//! Tests for `side`: remote https addresses win over paths, and path sides behave as before.

use std::path::Path;

use crate::args::{ArgError, SideSpec};
use crate::side_arg::side;

/// Parses `text` while no path exists.
fn none(text: &str) -> Result<SideSpec, ArgError> {
    side(text, Path::new("/w"), &|_| false)
}

/// Parses `text` while every path exists.
fn all(text: &str) -> Result<SideSpec, ArgError> {
    side(text, Path::new("/w"), &|_| true)
}

/// The expected remote side.
fn url(url: &str, rev: &str) -> Result<SideSpec, ArgError> {
    Ok(SideSpec::GitUrl {
        url: url.to_owned(),
        rev: rev.to_owned(),
    })
}

/// Addresses split into canonical url and revision, with or without existing paths.
#[test]
fn addresses_parse_into_url_and_rev() {
    let cases = [
        (
            "https://github.com/rust-lang/rust@master",
            "https://github.com/rust-lang/rust",
            "master",
        ),
        (
            "HTTPS://GitHub.com:443/owner/repo/",
            "https://github.com/owner/repo",
            "HEAD",
        ),
        (
            "https://github.com/owner/repo@refs/pull/12/head",
            "https://github.com/owner/repo",
            "refs/pull/12/head",
        ),
        (
            "https://gitlab.example.lan/team/app@refs/merge-requests/7/head",
            "https://gitlab.example.lan/team/app",
            "refs/merge-requests/7/head",
        ),
        (
            "https://codeberg.org/forgejo/forgejo@v9.0.0",
            "https://codeberg.org/forgejo/forgejo",
            "v9.0.0",
        ),
    ];
    for (text, address, rev) in cases {
        assert_eq!(none(text), url(address, rev), "{text}");
        assert_eq!(all(text), url(address, rev), "{text} with paths");
    }
}

/// Unsupported addresses fail with `BadUrl` carrying the typed text.
#[test]
fn bad_addresses_are_refused() {
    let cases = [
        "http://github.com/o/r",
        "ssh://git@github.com/o/r",
        "https://user@github.com/o/r@main",
        "https://github.com/o/r?x=1",
        "https://github.com/o/r#f",
        "https://github.com/o/r@a..b",
    ];
    for text in cases {
        assert_eq!(none(text), Err(ArgError::BadUrl(text.to_owned())), "{text}");
        assert_eq!(
            all(text),
            Err(ArgError::BadUrl(text.to_owned())),
            "{text} with paths"
        );
    }
}

/// Path and `repo@rev` sides keep their old behaviour.
#[test]
fn path_sides_are_unchanged() {
    let repo = |path: &Path| path == Path::new("/w/repo");
    assert_eq!(
        side("repo@main", Path::new("/w"), &repo),
        Ok(SideSpec::GitRev {
            repo: "/w/repo".into(),
            rev: "main".to_owned()
        })
    );
    assert_eq!(
        none(r"C:\src\app@main"),
        Ok(SideSpec::Path(Path::new("/w").join(r"C:\src\app@main")))
    );
}
