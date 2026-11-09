use super::*;
use std::path::PathBuf;

fn lock_error() -> io::Error {
    if cfg!(windows) {
        io::Error::from_raw_os_error(32)
    } else {
        io::Error::other("Failed to lock file: Resource temporarily unavailable")
    }
}

#[test]
fn classify_table() {
    let json = serde_json::from_str::<u8>("x").unwrap_err();
    let semver = Version::parse("not a version").unwrap_err();
    let path = PathBuf::from("pkg.nupkg");
    let table = [
        (Error::Io(io::Error::other("disk")), FeedError::Offline),
        (Error::Io(lock_error()), FeedError::Busy),
        (Error::Json(json), FeedError::BadFeed),
        (Error::Semver(semver), FeedError::BadFeed),
        (Error::InvalidPackage("x".into()), FeedError::BadFeed),
        (
            Error::ChecksumInvalid(path.clone(), "a".into(), "b".into()),
            FeedError::Corrupt,
        ),
        (Error::SizeInvalid(path, 1, 2), FeedError::Corrupt),
        (
            Error::Other("no asset".into()),
            FeedError::Other("no asset".into()),
        ),
    ];
    for (error, expected) in table {
        assert_eq!(classify(&error), expected, "{error}");
    }
}

#[test]
fn http_statuses_parse() {
    assert_eq!(parse_status("http status: 403"), Some(403));
    assert_eq!(parse_status("http status: 429"), Some(429));
    assert_eq!(parse_status("http status: 500"), Some(500));
    assert_eq!(parse_status("timeout: global"), None);
    assert_eq!(parse_status("http status: "), None);
}

#[test]
fn only_403_and_429_are_rate_limits() {
    for status in [403, 429] {
        assert_eq!(from_status(Some(status)), FeedError::RateLimited);
    }
    for status in [Some(404), Some(500), None] {
        assert_eq!(from_status(status), FeedError::Offline);
    }
}

#[test]
fn repo_is_a_github_url() {
    assert!(REPO.starts_with("https://github.com/"));
}

#[test]
fn module_holds_no_token() {
    let source = include_str!("feed.rs");
    let secrets = ["ghp_", "github_pat_", "Bearer "];
    assert!(secrets.iter().all(|word| !source.contains(word)));
    assert!(source.contains("GithubSource::new(REPO, None, true)"));
}

#[test]
fn release_feed_never_reads_the_test_override() {
    // A Windows checkout has CRLF line ends; the matches below expect LF.
    let source = include_str!("feed.rs").replace("\r\n", "\n");
    assert!(!source.contains("env::"));
    assert!(!source.contains("NAVO_UPDATE_FEED_DIR"));
    assert!(source.contains("#[cfg(feature = \"tester\")]\n        let source ="));
    let modules = include_str!("mod.rs").replace("\r\n", "\n");
    assert!(modules.contains("#[cfg(feature = \"tester\")]\npub(crate) mod test_feed;"));
}
