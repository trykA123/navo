//! The `git-remote` location form: `<url>@<rev>[/<inner>]` with the escaping of `sidewise-git` (NC224).

use sidewise_core::{Location, LocationError, ProviderId, ProviderPath};
use sidewise_git::location::{MAX_LOCATION_BYTES, ParseError, decode, encode_part};
use sidewise_git_address::{AddressError, RemoteRev, RemoteUrl};

/// The provider name of this form.
pub const PROVIDER: &str = "git-remote";

/// A parsed remote location: address, revision and path inside the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAt {
    /// Canonical repository address.
    pub url: RemoteUrl,
    /// Revision name, with escapes decoded.
    pub rev: RemoteRev,
    /// Path inside the repository, kept verbatim.
    pub inner: String,
}

/// Why a location is not a valid remote location.
#[derive(Debug, PartialEq, Eq)]
pub enum RemoteAtError {
    /// The provider is not `git-remote`.
    WrongProvider,
    /// The text is malformed: separator, escape, UTF-8 or length.
    Text(ParseError),
    /// The address or the revision was refused.
    Address(AddressError),
}

/// Encodes an address, a revision and an inner path as a `git-remote` location.
/// Fails with a `LocationError` only if the location cannot be built.
pub fn encode(url: &RemoteUrl, rev: &RemoteRev, inner: &str) -> Result<Location, LocationError> {
    let mut text = String::new();
    encode_part(url.as_str(), false, &mut text);
    text.push('@');
    encode_part(rev.as_str(), true, &mut text);
    if !inner.is_empty() {
        text.push('/');
        text.push_str(inner);
    }
    Location::with_provider(ProviderId::new(PROVIDER)?, ProviderPath::from_text(&text))
}

/// Parses a `git-remote` location back into address, revision and inner path.
/// Checks provider, UTF-8, length, separator, escapes, then address and revision.
pub fn parse(location: &Location) -> Result<RemoteAt, RemoteAtError> {
    if location.provider().as_str() != PROVIDER {
        return Err(RemoteAtError::WrongProvider);
    }
    let text = location
        .path()
        .as_str()
        .ok_or(RemoteAtError::Text(ParseError::InvalidUtf8))?;
    if text.len() > MAX_LOCATION_BYTES {
        return Err(RemoteAtError::Text(ParseError::TooLong));
    }
    let at = match text.find('@') {
        Some(at) if at > 0 => at,
        _ => return Err(RemoteAtError::Text(ParseError::MissingSeparator)),
    };
    let rest = &text[at + 1..];
    let (rev, inner) = rest.split_once('/').unwrap_or((rest, ""));
    let url = decode(&text[..at]).map_err(RemoteAtError::Text)?;
    let rev = decode(rev).map_err(RemoteAtError::Text)?;
    Ok(RemoteAt {
        url: RemoteUrl::parse(&url).map_err(RemoteAtError::Address)?,
        rev: RemoteRev::parse(&rev).map_err(RemoteAtError::Address)?,
        inner: inner.to_owned(),
    })
}

#[cfg(test)]
mod tests;
