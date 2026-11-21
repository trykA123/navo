//! Tests for the `git-remote` location form.

use super::*;

/// Builds a location with the `git-remote` provider and raw text.
fn raw(text: &str) -> Location {
    Location::with_provider(
        ProviderId::new(PROVIDER).unwrap(),
        ProviderPath::from_text(text),
    )
    .unwrap()
}

#[test]
fn round_trip_with_escapes_and_inner() {
    let url = RemoteUrl::parse("https://example.com/a@b%c.git").unwrap();
    let rev = RemoteRev::parse("refs/heads/we@ird%name").unwrap();
    let location = encode(&url, &rev, "src/lib.rs").unwrap();
    let back = parse(&location).unwrap();
    assert_eq!(
        back,
        RemoteAt {
            url,
            rev,
            inner: "src/lib.rs".into()
        }
    );
}

#[test]
fn round_trip_without_inner() {
    let url = RemoteUrl::parse("https://example.com/r").unwrap();
    let rev = RemoteRev::parse("main").unwrap();
    let back = parse(&encode(&url, &rev, "").unwrap()).unwrap();
    assert_eq!(back.inner, "");
    assert_eq!(back.rev, rev);
}

#[test]
fn malformed_text_is_refused() {
    assert_eq!(
        parse(&raw("https://example.com/r")),
        Err(RemoteAtError::Text(ParseError::MissingSeparator))
    );
    assert_eq!(
        parse(&raw("https://example.com/r@%zz")),
        Err(RemoteAtError::Text(ParseError::BadEscape))
    );
    assert_eq!(
        parse(&raw("http://example.com/r@main")),
        Err(RemoteAtError::Address(AddressError::Scheme))
    );
}
