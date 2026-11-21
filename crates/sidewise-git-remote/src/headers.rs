//! Pure request-line helpers for the WinHTTP backend (`winhttp_http`): the https URL splitter and
//! the header block gix hands over. Kept platform-free so they are unit-tested on every OS.

use std::io;

/// Splits `https://host[:port]/path?query` into host (IPv6 without brackets), port and path.
/// Fails with `InvalidInput` for another scheme, user info, an empty host or a bad port (0 too).
pub(crate) fn split_url(url: &str) -> io::Result<(&str, u16, &str)> {
    let bad = || io::Error::new(io::ErrorKind::InvalidInput, "not an https address");
    let scheme_ok = url
        .get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("https://"));
    let rest = url.get(8..).filter(|_| scheme_ok).ok_or_else(bad)?;
    let (authority, path) = rest.find('/').map_or((rest, "/"), |at| rest.split_at(at));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => {
            let port = port.parse().ok().filter(|&port: &u16| port != 0);
            (host, port.ok_or_else(bad)?)
        }
        _ => (authority, 443),
    };
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if host.is_empty() || authority.contains('@') {
        return Err(bad());
    }
    Ok((host, port, path))
}

/// Joins gix's header lines with CRLF for WinHTTP, dropping any `User-Agent` line so only the
/// Target's `Sidewise` agent is sent (NC218). Fails with `InvalidInput` when a line holds CR or LF.
pub(crate) fn header_block(lines: impl IntoIterator<Item = impl AsRef<str>>) -> io::Result<String> {
    let mut lines: Vec<String> = lines.into_iter().map(|h| h.as_ref().to_owned()).collect();
    if lines.iter().any(|line| line.contains(['\r', '\n'])) {
        let err = io::Error::new(io::ErrorKind::InvalidInput, "header line with a line break");
        return Err(err);
    }
    let is_agent = |line: &str| {
        let name = line.split_once(':').map_or(line, |(name, _)| name);
        name.trim().eq_ignore_ascii_case("user-agent")
    };
    lines.retain(|line| !is_agent(line));
    Ok(lines.join("\r\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_lines_are_dropped_in_any_case() {
        let lines = [
            "User-Agent: git/oxide-0.62.0",
            "Git-Protocol: version=2",
            "user-agent:x",
            "USER-AGENT : y",
            "Accept: */*",
        ];
        assert_eq!(
            header_block(lines).ok(),
            Some("Git-Protocol: version=2\r\nAccept: */*".to_owned())
        );
    }

    #[test]
    fn other_headers_are_kept_in_order() {
        let lines = ["Accept: a", "User-Agentish: b", "Content-Type: c"];
        assert_eq!(
            header_block(lines).ok(),
            Some("Accept: a\r\nUser-Agentish: b\r\nContent-Type: c".to_owned())
        );
    }

    #[test]
    fn line_breaks_are_refused() {
        for bad in ["Accept: a\r\nX: y", "Accept: a\nX", "User-Agent: a\rb"] {
            let err = header_block(["Accept: ok", bad]).err().map(|e| e.kind());
            assert_eq!(err, Some(io::ErrorKind::InvalidInput), "{bad:?}");
        }
    }

    #[test]
    fn splits_host_port_and_path() {
        let split = |url| split_url(url).ok();
        assert_eq!(
            split("https://h.example/a?b"),
            Some(("h.example", 443, "/a?b"))
        );
        assert_eq!(split("HTTPS://h:8443"), Some(("h", 8443, "/")));
        assert_eq!(split("https://[::1]:444/r"), Some(("::1", 444, "/r")));
        assert_eq!(split("https://[::1]/r"), Some(("::1", 443, "/r")));
    }

    #[test]
    fn refuses_bad_addresses_and_port_zero() {
        for url in [
            "http://h/",
            "https://",
            "https://u@h/",
            "https://h:x/",
            "https://h:70000/",
            "https://h:0/",
            "https://h:00/",
        ] {
            let err = split_url(url).err().map(|e| e.kind());
            assert_eq!(err, Some(io::ErrorKind::InvalidInput), "{url}");
        }
    }
}
