//! Version rules of the update check: tag parsing, release channels and the newest release.

pub use semver::Version;

/// The GitHub repository whose releases are checked.
pub const REPO: &str = "trykA123/sidewise";

/// Which releases a running build is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Only releases that are not pre-releases.
    Stable,
    /// Every non-draft release.
    Prerelease,
}

impl Channel {
    /// The channel of a running build: pre-release when its version has a `pre` part.
    pub fn of(running: &Version) -> Channel {
        if running.pre.is_empty() {
            Channel::Stable
        } else {
            Channel::Prerelease
        }
    }

    /// True when this channel offers release `v`; `prerelease` is the feed's own flag.
    pub fn offers(self, v: &Version, prerelease: bool) -> bool {
        match self {
            Channel::Prerelease => true,
            Channel::Stable => !prerelease && v.pre.is_empty(),
        }
    }
}

/// Parses a release tag: at most 64 bytes, one optional `v`, then strict semver.
pub fn parse_tag(tag: &str) -> Option<Version> {
    if tag.len() > 64 {
        return None;
    }
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// The highest release the running build's channel offers that is above `running`.
/// Precedence ignores build metadata. Releases are `(version, prerelease flag)`.
pub fn newest_above(running: &Version, releases: &[(Version, bool)]) -> Option<Version> {
    let channel = Channel::of(running);
    releases
        .iter()
        .filter(|(v, pre)| channel.offers(v, *pre))
        .map(|(v, _)| v)
        .filter(|v| v.cmp_precedence(running).is_gt())
        .max_by(|a, b| a.cmp_precedence(b))
        .cloned()
}

/// The web page of release `v` on GitHub.
pub fn release_page(v: &Version) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{v}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn a_tag_with_or_without_v_parses_equal() {
        assert_eq!(parse_tag("v0.1.0-alpha.2"), parse_tag("0.1.0-alpha.2"));
        assert_eq!(parse_tag("0.1.0-alpha.2"), Some(v("0.1.0-alpha.2")));
    }

    #[test]
    fn bad_tags_give_none() {
        let long = format!("1.0.{}", "0".repeat(61));
        assert_eq!(long.len(), 65);
        for tag in ["vv1.0.0", "1.0", "latest", long.as_str()] {
            assert_eq!(parse_tag(tag), None, "{tag}");
        }
    }

    #[test]
    fn a_prerelease_build_sees_the_next_alpha() {
        let r = [(v("0.1.0-alpha.2"), true)];
        assert_eq!(
            newest_above(&v("0.1.0-alpha.1"), &r),
            Some(v("0.1.0-alpha.2"))
        );
    }

    #[test]
    fn a_stable_build_ignores_prereleases() {
        let r = [(v("0.2.0-beta.1"), false), (v("0.2.0"), true)];
        assert_eq!(newest_above(&v("0.1.0"), &r), None);
        let r = [
            (v("0.2.0-beta.1"), false),
            (v("0.2.0"), true),
            (v("0.2.0"), false),
        ];
        assert_eq!(newest_above(&v("0.1.0"), &r), Some(v("0.2.0")));
    }

    #[test]
    fn build_metadata_is_not_newer() {
        let r = [(v("1.0.0+build.5"), false)];
        assert_eq!(newest_above(&v("1.0.0"), &r), None);
    }

    #[test]
    fn the_newest_wins_in_any_order() {
        let (a, b, c) = (v("0.1.0-alpha.2"), v("0.1.0-alpha.4"), v("0.1.0-alpha.3"));
        for r in [
            [(a.clone(), true), (b.clone(), true), (c.clone(), true)],
            [(c.clone(), true), (a.clone(), true), (b.clone(), true)],
            [(b.clone(), true), (c, true), (a, true)],
        ] {
            assert_eq!(newest_above(&v("0.1.0-alpha.1"), &r), Some(b.clone()));
        }
    }

    #[test]
    fn the_release_page_names_the_tag() {
        assert_eq!(
            release_page(&v("0.1.0-alpha.2")),
            "https://github.com/trykA123/sidewise/releases/tag/v0.1.0-alpha.2"
        );
    }
}
