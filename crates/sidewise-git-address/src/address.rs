//! Parses and canonicalises a typed remote address and revision (NC218, NC220); no network.

/// Why an address or revision was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    /// The scheme is not `https`.
    Scheme,
    /// The address holds a user name or password.
    UserInfo,
    /// The address holds a query or a fragment.
    QueryOrFragment,
    /// The host is missing or malformed, or the port is not 1 to 65535.
    Host,
    /// The path is missing, or holds an empty, `.` or `..` part, a space, a control character or `\`;
    /// or the whole address is longer than 2048 bytes.
    Path,
    /// The revision is not a valid partial reference name or holds `..`, `@{`, `~`, `^` or `:`.
    Rev,
}

/// Longest accepted address text, in bytes.
const MAX_ADDRESS_BYTES: usize = 2048;

/// Longest accepted revision text, in bytes.
const MAX_REV_BYTES: usize = 256;

/// A validated https repository address in its canonical form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteUrl(String);

impl RemoteUrl {
    /// Parses `https://host[:port]/path` into its canonical form, or says why it is refused.
    ///
    /// The canonical form is `https://host[:port]/path`: the scheme and host are lower-case,
    /// a default `:443` and one trailing `/` are dropped. The path keeps its case and any `.git`.
    pub fn parse(text: &str) -> Result<Self, AddressError> {
        if text.len() > MAX_ADDRESS_BYTES {
            return Err(AddressError::Path);
        }
        let (scheme, rest) = text.split_once("://").ok_or(AddressError::Scheme)?;
        if !scheme.eq_ignore_ascii_case("https") {
            return Err(AddressError::Scheme);
        }
        if rest.contains(['?', '#']) {
            return Err(AddressError::QueryOrFragment);
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.contains('@') {
            return Err(AddressError::UserInfo);
        }
        let host = canonical_host(authority)?;
        let path = path.strip_suffix('/').unwrap_or(path);
        let bad_part = |p: &str| p.is_empty() || p == "." || p == "..";
        let bad_char = |c: char| c.is_control() || c.is_whitespace() || c == '\\';
        if path.is_empty() || path.split('/').any(bad_part) || path.contains(bad_char) {
            return Err(AddressError::Path);
        }
        Ok(Self(format!("https://{host}/{path}")))
    }

    /// The canonical address text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The cache directory key: the first 32 hex digits of `blake3(canonical address)`.
    pub fn cache_key(&self) -> String {
        blake3::hash(self.0.as_bytes()).to_hex()[..32].to_string()
    }
}

/// Lower-cases `host[:port]`, drops the default port 443, and checks both parts.
fn canonical_host(authority: &str) -> Result<String, AddressError> {
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.contains(']') => (h, Some(p)),
        _ => (authority, None),
    };
    let name_ok = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
    let v6_ok = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .is_some_and(|h| {
            !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        });
    if host.is_empty() || !(name_ok || v6_ok) {
        return Err(AddressError::Host);
    }
    let host = host.to_ascii_lowercase();
    match port {
        None => Ok(host),
        Some(p) => match p.parse::<u16>() {
            Ok(443) if p.bytes().all(|b| b.is_ascii_digit()) => Ok(host),
            Ok(n) if n > 0 && p.bytes().all(|b| b.is_ascii_digit()) => Ok(format!("{host}:{n}")),
            _ => Err(AddressError::Host),
        },
    }
}

/// Checks the NC220 rules by hand: 1 to 256 bytes, no `..`, `@{`, control or space, none of
/// `~^:?*[\\`, not `@` alone, no leading `-` or `/`, no trailing `/` or `.`, no empty part, and
/// no part that starts with `.` or ends with `.lock`.
fn rev_ok(t: &str) -> bool {
    let bad_char = |c: char| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c);
    !(t.contains("..")
        || t.contains("@{")
        || t.contains(bad_char)
        || t.starts_with(['-', '/'])
        || t.ends_with('/')
        || t.ends_with('.')
        || t == "@"
        || t.len() > MAX_REV_BYTES
        || t.contains("//")
        || t.split('/')
            .any(|p| p.starts_with('.') || p.ends_with(".lock")))
}

/// A validated revision: a branch, tag, full reference name or `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteRev(String);

impl RemoteRev {
    /// Checks a revision; an empty text means `HEAD`. Fails with [`AddressError::Rev`].
    pub fn parse(text: &str) -> Result<Self, AddressError> {
        if text.is_empty() {
            return Ok(Self("HEAD".to_owned()));
        }
        if rev_ok(text) {
            Ok(Self(text.to_owned()))
        } else {
            Err(AddressError::Rev)
        }
    }

    /// The revision text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests;
