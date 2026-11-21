//! Reads GitHub's release list defensively: only tag, draft and prerelease, each entry alone.

use crate::version::{Version, parse_tag};
use serde::Deserialize;

/// The largest feed body that is read, in bytes.
pub const MAX_FEED_BYTES: usize = 512 * 1024;

/// The most release entries that are looked at.
pub const MAX_RELEASES: usize = 50;

/// Why a feed body could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum FeedError {
    /// The body is larger than `MAX_FEED_BYTES`.
    TooLarge,
    /// The body is not a JSON array.
    NotJson,
}

/// One release as the feed gives it: tag, draft flag and pre-release flag.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

/// Reads a feed body into `(version, prerelease)` pairs in feed order.
/// `TooLarge` over `MAX_FEED_BYTES`; `NotJson` when the body is not a JSON array.
pub fn parse_feed(body: &[u8]) -> Result<Vec<(Version, bool)>, FeedError> {
    if body.len() > MAX_FEED_BYTES {
        return Err(FeedError::TooLarge);
    }
    let entries =
        serde_json::from_slice::<Vec<serde_json::Value>>(body).map_err(|_| FeedError::NotJson)?;
    let mut releases = Vec::new();
    for entry in entries.into_iter().take(MAX_RELEASES) {
        let Ok(raw) = serde_json::from_value::<RawRelease>(entry) else {
            continue;
        };
        if raw.draft {
            continue;
        }
        if let Some(version) = parse_tag(&raw.tag_name) {
            releases.push((version, raw.prerelease));
        }
    }
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn releases(body: &str) -> Result<Vec<(Version, bool)>, FeedError> {
        parse_feed(body.as_bytes())
    }

    #[test]
    fn a_real_shaped_feed_gives_both_entries() {
        let body = r#"[
          {
            "url": "https://api.github.com/repos/trykA123/sidewise/releases/1",
            "tag_name": "v0.1.0-alpha.2",
            "draft": false,
            "prerelease": true,
            "author": {"login": "trykA123", "id": 1},
            "assets": [{"name": "Sidewise.zip", "size": 12345}]
          },
          {
            "tag_name": "0.1.0",
            "draft": false,
            "prerelease": false,
            "assets": []
          }
        ]"#;
        assert_eq!(
            releases(body),
            Ok(vec![(v("0.1.0-alpha.2"), true), (v("0.1.0"), false)])
        );
    }

    #[test]
    fn a_draft_is_skipped() {
        let body = r#"[
          {"tag_name": "v0.2.0", "draft": true, "prerelease": false},
          {"tag_name": "v0.1.0", "draft": false, "prerelease": false}
        ]"#;
        assert_eq!(releases(body), Ok(vec![(v("0.1.0"), false)]));
    }

    #[test]
    fn a_number_tag_is_skipped_and_the_next_kept() {
        let body = r#"[
          {"tag_name": 42, "draft": false, "prerelease": false},
          {"tag_name": "v0.1.0", "draft": false, "prerelease": false}
        ]"#;
        assert_eq!(releases(body), Ok(vec![(v("0.1.0"), false)]));
    }

    #[test]
    fn an_object_or_garbage_is_not_json() {
        assert_eq!(releases("{}"), Err(FeedError::NotJson));
        assert_eq!(releases("not json"), Err(FeedError::NotJson));
    }

    #[test]
    fn a_body_over_the_cap_is_too_large() {
        let body = vec![b' '; MAX_FEED_BYTES + 1];
        assert_eq!(parse_feed(&body), Err(FeedError::TooLarge));
    }

    #[test]
    fn only_the_first_fifty_entries_are_looked_at() {
        let entries: Vec<String> = (0..60)
            .map(|i| {
                format!(r#"{{"tag_name":"v0.1.0-alpha.{i}","draft":false,"prerelease":true}}"#)
            })
            .collect();
        let body = format!("[{}]", entries.join(","));
        let got = releases(&body).unwrap();
        assert_eq!(got.len(), MAX_RELEASES);
        assert_eq!(got.last(), Some(&(v("0.1.0-alpha.49"), true)));
    }
}
