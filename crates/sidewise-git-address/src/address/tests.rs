//! Tests for `RemoteUrl` and `RemoteRev` (RG1).

use super::*;

fn url(t: &str) -> Result<String, AddressError> {
    RemoteUrl::parse(t).map(|u| u.as_str().to_owned())
}

#[test]
fn accepted_forms_canonicalise() {
    let cases = [
        (
            "https://github.com/rust-lang/rust",
            "https://github.com/rust-lang/rust",
        ),
        (
            "HTTPS://GitHub.com/Rust-Lang/rust/",
            "https://github.com/Rust-Lang/rust",
        ),
        (
            "https://codeberg.org/forgejo/forgejo.git",
            "https://codeberg.org/forgejo/forgejo.git",
        ),
        (
            "https://gitlab.example.lan/team/app",
            "https://gitlab.example.lan/team/app",
        ),
        (
            "https://192.168.1.5:8443/a/b",
            "https://192.168.1.5:8443/a/b",
        ),
        ("https://host:443/a", "https://host/a"),
        ("https://[::1]:8443/a", "https://[::1]:8443/a"),
        ("https://[fe80::1]/a", "https://[fe80::1]/a"),
    ];
    for (input, want) in cases {
        assert_eq!(url(input).as_deref(), Ok(want), "{input}");
    }
}

#[test]
fn refused_addresses() {
    let cases = [
        ("http://github.com/a/b", AddressError::Scheme),
        ("ssh://github.com/a/b", AddressError::Scheme),
        ("github.com/a/b", AddressError::Scheme),
        ("https://me@github.com/a/b", AddressError::UserInfo),
        ("https://me:pw@github.com/a/b", AddressError::UserInfo),
        ("https://github.com/a/b?x=1", AddressError::QueryOrFragment),
        ("https://github.com/a/b#frag", AddressError::QueryOrFragment),
        ("https:///a/b", AddressError::Host),
        ("https://host:0/a", AddressError::Host),
        ("https://host:99999/a", AddressError::Host),
        ("https://host:+80/a", AddressError::Host),
        ("https://ho st/a", AddressError::Host),
        ("https://github.com", AddressError::Path),
        ("https://github.com/", AddressError::Path),
        ("https://github.com/a/b//", AddressError::Path),
        ("https://github.com/a//b", AddressError::Path),
        ("https://github.com/a/../b", AddressError::Path),
        ("https://github.com/a b", AddressError::Path),
        ("https://github.com/a\\b", AddressError::Path),
    ];
    for (input, want) in cases {
        assert_eq!(url(input), Err(want), "{input}");
    }
}

#[test]
fn trailing_slash_and_host_case_share_a_cache_key() {
    let a = RemoteUrl::parse("https://Example.com/x/y").unwrap();
    let b = RemoteUrl::parse("https://example.com/x/y/").unwrap();
    assert_eq!(a.cache_key(), b.cache_key());
    assert_eq!(a.cache_key().len(), 32);
    assert!(a.cache_key().bytes().all(|c| c.is_ascii_hexdigit()));
    let other = RemoteUrl::parse("https://example.com/x/z").unwrap();
    assert_ne!(a.cache_key(), other.cache_key());
}

#[test]
fn cache_key_is_blake3_prefix() {
    let u = RemoteUrl::parse("https://example.com/x/y").unwrap();
    let full = blake3::hash(b"https://example.com/x/y").to_hex();
    assert_eq!(u.cache_key(), full[..32]);
}

#[test]
fn rev_accepts_and_defaults() {
    for ok in [
        "master",
        "v9.0.0",
        "refs/pull/12/head",
        "refs/merge-requests/7/head",
        "HEAD",
        "feature/x",
    ] {
        assert_eq!(
            RemoteRev::parse(ok).map(|r| r.as_str().to_owned()),
            Ok(ok.to_owned())
        );
    }
    assert_eq!(RemoteRev::parse("").unwrap().as_str(), "HEAD");
}

#[test]
fn rev_refuses_unsafe_forms() {
    for bad in [
        "a..b",
        "a@{1}",
        "a~1",
        "a^",
        "a:b",
        "a b",
        "a//b",
        "a.lock",
        "-x",
        "/a",
        "a/",
        "a\x01",
        "a*",
        "a\\b",
        ".a",
        "a/.b",
        "a.",
        "@",
        "a/b.lock/c",
        "a.lock/b",
    ] {
        assert_eq!(RemoteRev::parse(bad), Err(AddressError::Rev), "{bad:?}");
    }
}

#[test]
fn rev_length_cap_is_256_bytes() {
    assert!(RemoteRev::parse(&"a".repeat(256)).is_ok());
    assert_eq!(RemoteRev::parse(&"a".repeat(257)), Err(AddressError::Rev));
}

#[test]
fn address_length_cap_is_2048_bytes() {
    let head = "https://h.example/";
    let ok = format!("{head}{}", "a".repeat(2048 - head.len()));
    assert!(RemoteUrl::parse(&ok).is_ok());
    assert_eq!(RemoteUrl::parse(&format!("{ok}a")), Err(AddressError::Path));
}
