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
}
