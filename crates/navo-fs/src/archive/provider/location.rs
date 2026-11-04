use std::path::{Path, PathBuf};

use navo_core::{Location, LocationError, ProviderPath};

use super::provider_id;

pub(super) const MAX_LOCATION_BYTES: usize = 32 * 1024;
pub(super) const MAX_COMPONENTS: usize = 1024;

pub(super) struct ArchiveLocation {
    pub archive: PathBuf,
    pub inner: Vec<u8>,
    pub location: Location,
}

#[derive(Debug)]
pub(super) enum ParseError {
    WrongProvider,
    MissingSeparator,
    EmptyArchive,
    BadEscape,
    InvalidUtf8,
    TooLong,
    TooManyComponents,
}

pub fn location(archive: &Path, inner: &str) -> Result<Location, LocationError> {
    build(archive, inner.trim_start_matches('/'), false)
}

pub fn raw_location(archive: &Path, inner: &str) -> Result<Location, LocationError> {
    build(archive, inner, true)
}

fn build(archive: &Path, inner: &str, raw: bool) -> Result<Location, LocationError> {
    let archive = archive.to_str().ok_or(LocationError::BadSegment)?;
    let mut text = String::new();
    encode(archive, &mut text);
    text.push('#');
    if raw {
        text.push('!');
        encode_raw(inner, &mut text);
    } else if !inner.is_empty() {
        text.push('/');
        text.push_str(inner);
    }
    let provider = super::provider_id();
    Location::with_provider(provider, ProviderPath::from_text(&text))
}

pub(super) fn parse(location: &Location) -> Result<ArchiveLocation, ParseError> {
    if location.provider() != &provider_id() {
        return Err(ParseError::WrongProvider);
    }
    let text = location.path().to_text();
    if text.len() > MAX_LOCATION_BYTES {
        return Err(ParseError::TooLong);
    }
    let at = text.find('#').ok_or(ParseError::MissingSeparator)?;
    if at == 0 {
        return Err(ParseError::EmptyArchive);
    }
    let archive = decode(&text[..at])?;
    if archive.is_empty() {
        return Err(ParseError::EmptyArchive);
    }
    let rest = &text[at + 1..];
    let inner = match rest.strip_prefix('!') {
        Some(raw) => decode(raw)?.into_bytes(),
        None => inner_bytes(rest.trim_start_matches('/')),
    };
    if Path::new(&archive).components().count() + components_bytes(&inner) > MAX_COMPONENTS {
        return Err(ParseError::TooManyComponents);
    }
    Ok(ArchiveLocation {
        archive: PathBuf::from(archive),
        inner,
        location: location.clone(),
    })
}

fn encode(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '/' => out.push_str("%2F"),
            '\\' => out.push_str("%5C"),
            other => out.push(other),
        }
    }
}

fn encode_raw(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            '\0' => out.push_str("%00"),
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

fn components_bytes(path: &[u8]) -> usize {
    path.split(|byte| *byte == b'/')
        .filter(|part| !part.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_and_inner_round_trip_apart() {
        let archive = Path::new("/tmp/re%po#rt/arch ive.zip");
        let location = location(archive, "docs/read#me.txt").unwrap();
        let parsed = parse(&location).unwrap();
        assert_eq!(parsed.archive, archive);
        assert_eq!(parsed.inner, b"docs/read#me.txt");
        assert!(location.as_str().contains("%25"));
        assert!(location.as_str().contains("%23"));
        assert!(location.as_str().contains("%2F"));
    }

    #[test]
    fn windows_drive_paths_round_trip() {
        let archive = Path::new(r"C:\src\a rchive.7z");
        let location = location(archive, "src/lib.rs").unwrap();
        let parsed = parse(&location).unwrap();
        assert_eq!(parsed.archive, archive);
        assert_eq!(parsed.inner, b"src/lib.rs");
        assert!(location.as_str().contains("%5C"));
    }

    #[test]
    fn joining_a_name_stays_in_the_inner_path() {
        let archive = Path::new("/tmp/a.zip");
        let root = location(archive, "").unwrap();
        let docs = root.join(&ProviderPath::from_text("docs")).unwrap();
        assert_eq!(docs, location(archive, "docs").unwrap());
        let file = docs.join(&ProviderPath::from_text("a#b")).unwrap();
        assert_eq!(file, location(archive, "docs/a#b").unwrap());
        assert_eq!(parse(&file).unwrap().inner, b"docs/a#b");
    }

    #[test]
    fn a_raw_location_keeps_unsafe_names() {
        let archive = Path::new("/tmp/a.zip");
        for name in [
            "../evil",
            "/absolute",
            r"C:\drive",
            "a/b",
            "nul\0name",
            "100% safe",
        ] {
            let raw = raw_location(archive, name).unwrap();
            assert_eq!(parse(&raw).unwrap().inner, name.as_bytes());
        }
    }

    #[test]
    fn a_bad_escape_or_a_missing_separator_is_refused() {
        let provider = super::super::provider_id();
        let text = ProviderPath::from_text("/tmp/a.zip%zz#");
        let bad = Location::with_provider(provider.clone(), text).unwrap();
        assert!(matches!(parse(&bad), Err(ParseError::BadEscape)));
        let missing =
            Location::with_provider(provider, ProviderPath::from_text("/tmp/a.zip")).unwrap();
        assert!(matches!(parse(&missing), Err(ParseError::MissingSeparator)));
    }

    #[test]
    fn an_overlong_location_is_refused() {
        let provider = super::super::provider_id();
        let text = format!("{}#", "a".repeat(MAX_LOCATION_BYTES));
        let location = Location::with_provider(provider, ProviderPath::from_text(&text)).unwrap();
        assert!(matches!(parse(&location), Err(ParseError::TooLong)));
    }
}
