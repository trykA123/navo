use std::path::{Path, PathBuf};

use rusty_core::{Location, LocationError, ProviderPath};

use super::provider_id;

pub(super) const MAX_LOCATION_BYTES: usize = 32 * 1024;
pub(super) const MAX_COMPONENTS: usize = 1024;

pub(super) struct GitLocation {
    pub repo: PathBuf,
    pub rev: String,
    pub inner: Vec<u8>,
    pub location: Location,
}

#[derive(Debug)]
pub(super) enum ParseError {
    WrongProvider,
    MissingSeparator,
    EmptyRev,
    BadEscape,
    InvalidUtf8,
    TooLong,
    TooManyComponents,
}

pub fn location(repo: &Path, rev: &str, inner: &str) -> Result<Location, LocationError> {
    if rev.is_empty() {
        return Err(LocationError::BadSegment);
    }
    let repo = repo.to_str().ok_or(LocationError::BadSegment)?;
    let mut text = String::new();
    encode(repo, false, &mut text);
    text.push('@');
    encode(rev, true, &mut text);
    let inner = inner.trim_start_matches('/');
    if !inner.is_empty() {
        text.push('/');
        text.push_str(inner);
    }
    let provider = rusty_core::ProviderId::new("git")?;
    Location::with_provider(provider, ProviderPath::from_text(&text))
}

pub(super) fn parse(location: &Location) -> Result<GitLocation, ParseError> {
    if location.provider() != &provider_id() {
        return Err(ParseError::WrongProvider);
    }
    let text = location.path().to_text();
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
    let inner = inner_bytes(inner);
    if components(&repo) + components_bytes(&inner) > MAX_COMPONENTS {
        return Err(ParseError::TooManyComponents);
    }
    Ok(GitLocation {
        repo: PathBuf::from(repo),
        rev,
        inner,
        location: location.clone(),
    })
}

fn encode(text: &str, slash: bool, out: &mut String) {
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            '@' => out.push_str("%40"),
            '/' if slash => out.push_str("%2F"),
            other => out.push(other),
        }
    }
}

fn decode(text: &str) -> Result<String, ParseError> {
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

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn inner_bytes(text: &str) -> Vec<u8> {
    let path = ProviderPath::from_text(text);
    path.to_bytes()
        .unwrap_or_else(|| path.to_text().into_bytes())
}

fn components(repo: &str) -> usize {
    Path::new(repo).components().count()
}

fn components_bytes(path: &[u8]) -> usize {
    path.split(|byte| *byte == b'/')
        .filter(|part| !part.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_and_percent_and_slashes_round_trip() {
        let repo = Path::new("/tmp/re@po%");
        let location = location(repo, "feature/x", "dir@HEAD/secret").unwrap();
        let parsed = parse(&location).unwrap();
        assert_eq!(parsed.repo, repo);
        assert_eq!(parsed.rev, "feature/x");
        assert_eq!(parsed.inner, b"dir@HEAD/secret");
        assert!(location.as_str().contains("%40"));
        assert!(location.as_str().contains("%2F"));
    }

    #[test]
    fn windows_drive_paths_round_trip() {
        let repo = Path::new(r"C:\src\re@po");
        let location = location(repo, "main", "src/lib.rs").unwrap();
        let parsed = parse(&location).unwrap();
        assert_eq!(parsed.repo, repo);
        assert_eq!(parsed.rev, "main");
        assert_eq!(parsed.inner, b"src/lib.rs");
    }

    #[test]
    fn the_first_unescaped_at_ends_the_repo() {
        let provider = rusty_core::ProviderId::new("git").unwrap();
        let location = Location::with_provider(
            provider,
            ProviderPath::from_text("repo@HEAD/dir@HEAD/secret"),
        )
        .unwrap();
        let parsed = parse(&location).unwrap();
        assert_eq!(parsed.repo, Path::new("repo"));
        assert_eq!(parsed.rev, "HEAD");
        assert_eq!(parsed.inner, b"dir@HEAD/secret");
    }
}
