use crate::{FeedError, Found, Offer, Version};

/// Parses a semver string; anything else is `BadFeed`.
pub fn parse_version(text: &str) -> Result<Version, FeedError> {
    Version::parse(text.trim()).map_err(|_| FeedError::BadFeed)
}

/// `Newer` only when the found release is strictly above `current`.
pub fn offer(current: &Version, found: Found) -> Offer {
    match found {
        Found::Release(release) if release.version > *current => Offer::Newer(release),
        _ => Offer::NotNewer,
    }
}

/// Strips an optional leading `'v'` and parses as semver.
fn normalize(text: &str) -> Result<Version, FeedError> {
    parse_version(text.trim().strip_prefix('v').unwrap_or(text.trim()))
}

/// Returns `true` when `skipped` matches `offered`, normalizing an optional
/// leading `'v'` (e.g. `"v0.9.3"` == `"0.9.3"`).
///
/// Returns `false` when either side is not valid semver.
#[must_use]
pub fn should_skip_update(skipped: Option<&str>, offered: &str) -> bool {
    let Some(skipped) = skipped else {
        return false;
    };
    match (normalize(skipped), normalize(offered)) {
        (Ok(s), Ok(o)) => s == o,
        _ => false,
    }
}

/// Clears `skipped` when a strictly newer semver arrives.
///
/// Returns `true` when `skipped` was cleared. Keeps the skip when
/// `current_or_offered` is the same version or older than the skipped one.
/// Returns `false` when either side is not valid semver (leaves `skipped` unchanged).
pub fn clear_skipped_if_newer(skipped: &mut Option<String>, current_or_offered: &str) -> bool {
    let Some(skipped_text) = skipped.as_deref() else {
        return false;
    };
    let Ok(skipped_ver) = normalize(skipped_text) else {
        return false;
    };
    let Ok(offered_ver) = normalize(current_or_offered) else {
        return false;
    };
    if offered_ver > skipped_ver {
        *skipped = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Release;

    fn v(text: &str) -> Version {
        parse_version(text).expect("valid version")
    }

    fn release(text: &str) -> Found {
        Found::Release(Release {
            version: v(text),
            size: 1,
        })
    }

    #[test]
    fn version_ordering_chain() {
        let chain = [
            "0.0.1-alpha.20261030c",
            "0.0.1-alpha.20261030d",
            "0.0.1-alpha.20261102a",
            "0.0.1-beta.20261101a",
            "0.1.0",
        ];
        for pair in chain.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn version_newer_is_offered() {
        let found = release("0.1.0");
        let got = offer(&v("0.0.1-beta.20261101a"), found.clone());
        assert!(matches!(got, Offer::Newer(r) if r.version == v("0.1.0")));
    }

    #[test]
    fn version_equal_and_older_are_not_newer() {
        assert_eq!(offer(&v("0.1.0"), release("0.1.0")), Offer::NotNewer);
        assert_eq!(
            offer(&v("0.1.0"), release("0.0.1-alpha.20261030c")),
            Offer::NotNewer
        );
        assert_eq!(offer(&v("0.1.0"), Found::Nothing), Offer::NotNewer);
    }

    #[test]
    fn version_bad_string_is_bad_feed() {
        assert_eq!(parse_version("nope"), Err(FeedError::BadFeed));
        assert_eq!(parse_version(""), Err(FeedError::BadFeed));
    }

    #[test]
    fn skip_matches_exact_version() {
        assert!(should_skip_update(Some("0.9.3"), "0.9.3"));
    }

    #[test]
    fn skip_normalizes_leading_v() {
        assert!(should_skip_update(Some("v0.9.3"), "0.9.3"));
        assert!(should_skip_update(Some("0.9.3"), "v0.9.3"));
        assert!(should_skip_update(Some("v0.9.3"), "v0.9.3"));
    }

    #[test]
    fn skip_does_not_match_different_version() {
        assert!(!should_skip_update(Some("0.9.3"), "0.9.4"));
    }

    #[test]
    fn skip_none_never_skips() {
        assert!(!should_skip_update(None, "0.9.3"));
    }

    #[test]
    fn skip_invalid_semver_returns_false() {
        assert!(!should_skip_update(Some("not-a-version"), "0.9.3"));
        assert!(!should_skip_update(Some("0.9.3"), "garbage"));
    }

    #[test]
    fn clear_skipped_when_strictly_newer() {
        let mut skipped = Some("0.9.3".to_owned());
        assert!(clear_skipped_if_newer(&mut skipped, "0.9.4"));
        assert_eq!(skipped, None);
    }

    #[test]
    fn clear_skipped_keeps_on_same_version() {
        let mut skipped = Some("0.9.3".to_owned());
        assert!(!clear_skipped_if_newer(&mut skipped, "0.9.3"));
        assert_eq!(skipped, Some("0.9.3".to_owned()));
    }

    #[test]
    fn clear_skipped_keeps_on_older_version() {
        let mut skipped = Some("0.9.3".to_owned());
        assert!(!clear_skipped_if_newer(&mut skipped, "0.9.2"));
        assert_eq!(skipped, Some("0.9.3".to_owned()));
    }

    #[test]
    fn clear_skipped_normalizes_leading_v() {
        let mut skipped = Some("v0.9.3".to_owned());
        assert!(clear_skipped_if_newer(&mut skipped, "0.9.4"));
        assert_eq!(skipped, None);
    }

    #[test]
    fn clear_skipped_noop_when_none() {
        let mut skipped = None;
        assert!(!clear_skipped_if_newer(&mut skipped, "0.9.4"));
        assert_eq!(skipped, None);
    }

    #[test]
    fn clear_skipped_noop_on_invalid_semver() {
        let mut skipped = Some("0.9.3".to_owned());
        assert!(!clear_skipped_if_newer(&mut skipped, "garbage"));
        assert_eq!(skipped, Some("0.9.3".to_owned()));
    }

    #[test]
    fn auto_check_skips_then_unskips_on_strictly_newer() {
        let mut skipped = Some("0.9.3".to_owned());

        assert!(should_skip_update(skipped.as_deref(), "0.9.3"));

        assert!(clear_skipped_if_newer(&mut skipped, "0.9.4"));
        assert_eq!(skipped, None);

        assert!(!should_skip_update(skipped.as_deref(), "0.9.4"));
    }
}
