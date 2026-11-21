//! The Git location form: a repository, a revision and an optional path inside it.
//! Encodes to and parses from the `git` and `git-checkout` providers.

use std::path::{Path, PathBuf};

use sidewise_core::{Location, LocationError, ProviderId, ProviderPath};

/// Largest location text accepted, in bytes.
pub const MAX_LOCATION_BYTES: usize = 32 * 1024;

/// The provider form of a Git location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    /// Bytes stored by Sidewise, not a working checkout.
    Stored,
    /// A path in a working checkout.
    Checkout,
}

impl Form {
    /// Returns the provider name for this form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "git",
            Self::Checkout => "git-checkout",
        }
    }

    /// Returns the provider id for this form, or a provider error.
    pub fn provider(self) -> Result<ProviderId, LocationError> {
        ProviderId::new(self.as_str())
    }

    /// Returns the form named by `provider`, or `None` for any other provider.
    #[must_use]
    pub fn of(provider: &ProviderId) -> Option<Self> {
        if provider.as_str() == Self::Stored.as_str() {
            Some(Self::Stored)
        } else if provider.as_str() == Self::Checkout.as_str() {
            Some(Self::Checkout)
        } else {
            None
        }
    }
}

/// A parsed Git location: repository, revision, path inside and form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitAt {
    /// Repository path, with escapes decoded.
    pub repo: PathBuf,
    /// Revision name, with escapes decoded.
    pub rev: String,
    /// Path inside the repository, kept verbatim.
    pub inner: String,
    /// Location form named by the provider.
    pub form: Form,
}

/// Why a location text is not a Git location.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The provider is neither `git` nor `git-checkout`.
    WrongProvider,
    /// There is no `@`, or the repository part is empty.
    MissingSeparator,
    /// The revision is empty after decoding.
    EmptyRev,
    /// A `%` is not followed by two hex digits.
    BadEscape,
    /// The path or a decoded part is not valid UTF-8.
    InvalidUtf8,
    /// The location text is over `MAX_LOCATION_BYTES`.
    TooLong,
}

/// Encodes a repository and revision as a stored or checkout location.
/// Fails with `BadSegment` for an empty revision or a non-UTF-8 repository path.
pub fn encode(repo: &Path, rev: &str, form: Form) -> Result<Location, LocationError> {
    if rev.is_empty() {
        return Err(LocationError::BadSegment);
    }
    let repo = repo.to_str().ok_or(LocationError::BadSegment)?;
    let mut text = String::new();
    encode_part(repo, false, &mut text);
    text.push('@');
    encode_part(rev, true, &mut text);
    Location::with_provider(form.provider()?, ProviderPath::from_text(&text))
}

/// Appends `text` to `out`, escaping `%` and `@`, and `/` when `slash` is set.
pub fn encode_part(text: &str, slash: bool, out: &mut String) {
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            '@' => out.push_str("%40"),
            '/' if slash => out.push_str("%2F"),
            other => out.push(other),
        }
    }
}

/// Parses a Git location back into its repository, revision, inner path and form.
/// Checks the provider, UTF-8 text, length, separator, escapes and revision in that order.
pub fn parse(location: &Location) -> Result<GitAt, ParseError> {
    let form = Form::of(location.provider()).ok_or(ParseError::WrongProvider)?;
    let text = location.path().as_str().ok_or(ParseError::InvalidUtf8)?;
    if text.len() > MAX_LOCATION_BYTES {
        return Err(ParseError::TooLong);
    }
    let at = text.find('@').ok_or(ParseError::MissingSeparator)?;
    if at == 0 {
        return Err(ParseError::MissingSeparator);
    }
    let repo = decode(&text[..at])?;
    let rest = &text[at + 1..];
    let (rev, inner) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash + 1..]),
        None => (rest, ""),
    };
    let rev = decode(rev)?;
    if rev.is_empty() {
        return Err(ParseError::EmptyRev);
    }
    Ok(GitAt {
        repo: PathBuf::from(repo),
        rev,
        inner: inner.to_owned(),
        form,
    })
}

/// Percent-decodes `text` into a UTF-8 string.
/// Fails with `BadEscape` for a malformed escape and `InvalidUtf8` for non-UTF-8 bytes.
pub fn decode(text: &str) -> Result<String, ParseError> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let high = bytes
                .get(at + 1)
                .copied()
                .and_then(hex)
                .ok_or(ParseError::BadEscape)?;
            let low = bytes
                .get(at + 2)
                .copied()
                .and_then(hex)
                .ok_or(ParseError::BadEscape)?;
            decoded.push((high << 4) | low);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ParseError::InvalidUtf8)
}

/// Returns the value of a hex digit, or `None` for any other byte.
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sidewise_core::{Location, ProviderId, ProviderPath};

    use super::{Form, GitAt, ParseError, encode, parse};

    /// Builds a stored `git` location around `text`.
    fn location(text: &str) -> Location {
        Location::with_provider(
            Form::Stored.provider().expect("provider"),
            ProviderPath::from_text(text),
        )
        .expect("location")
    }

    /// Parses a stored `git` location built around `text`.
    fn parse_text(text: &str) -> Result<GitAt, ParseError> {
        parse(&location(text))
    }

    #[test]
    fn at_percent_and_slashes_round_trip() {
        let repo = Path::new("/tmp/re@po%");
        let stored = encode(repo, "feature/x", Form::Stored).expect("encode");
        let text = stored
            .join(&ProviderPath::from_text("dir@HEAD"))
            .expect("dir")
            .join(&ProviderPath::from_text("secret"))
            .expect("secret");
        let parsed = parse(&text).expect("parse");
        assert_eq!(parsed.repo, repo);
        assert_eq!(parsed.rev, "feature/x");
        assert_eq!(parsed.inner, "dir@HEAD/secret");
        assert_eq!(parsed.form, Form::Stored);
        assert!(text.as_str().contains("%40"));
        assert!(text.as_str().contains("%2F"));
    }

    #[test]
    fn windows_drive_paths_round_trip() {
        let repo = Path::new(r"C:\src\re@po");
        let stored = encode(repo, "main", Form::Stored).expect("encode");
        let parsed = parse(&stored).expect("parse");
        assert_eq!(parsed.repo, repo);
        assert_eq!(parsed.rev, "main");
        assert_eq!(parsed.inner, "");
    }

    #[test]
    fn first_at_ends_the_repo() {
        let parsed = parse_text("repo@HEAD/dir@HEAD/secret").expect("parse");
        assert_eq!(parsed.repo, Path::new("repo"));
        assert_eq!(parsed.rev, "HEAD");
        assert_eq!(parsed.inner, "dir@HEAD/secret");
    }

    #[test]
    fn local_location_is_wrong_provider() {
        let local =
            Location::with_provider(ProviderId::local(), ProviderPath::from_text("repo@HEAD"))
                .expect("location");
        assert_eq!(parse(&local), Err(ParseError::WrongProvider));
    }

    #[test]
    fn an_empty_revision_is_reported() {
        assert_eq!(parse_text("repo@"), Err(ParseError::EmptyRev));
    }

    #[test]
    fn a_malformed_escape_is_reported() {
        assert_eq!(parse_text("repo@%4"), Err(ParseError::BadEscape));
    }

    #[test]
    fn a_non_utf8_escape_is_reported() {
        assert_eq!(parse_text("repo@%FF"), Err(ParseError::InvalidUtf8));
    }

    #[test]
    fn a_location_over_the_limit_is_too_long() {
        let text = format!("repo@{}", "a".repeat(33 * 1024));
        assert_eq!(parse_text(&text), Err(ParseError::TooLong));
    }

    #[test]
    fn form_of_round_trips_both_providers() {
        assert_eq!(
            Form::of(&Form::Stored.provider().expect("provider")),
            Some(Form::Stored)
        );
        assert_eq!(
            Form::of(&Form::Checkout.provider().expect("provider")),
            Some(Form::Checkout)
        );
        assert_eq!(Form::of(&ProviderId::local()), None);
    }

    #[test]
    fn a_joined_file_parses_as_inner() {
        let stored = encode(Path::new("/tmp/repo"), "HEAD", Form::Stored).expect("encode");
        let file = stored.join(&ProviderPath::from_text("a.rs")).expect("join");
        let parsed = parse(&file).expect("parse");
        assert_eq!(parsed.inner, "a.rs");
    }
}
