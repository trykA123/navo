//! Remote Git sides for Sidewise: the `git-remote` location form, the fetch into a local cache and,
//! on Windows, its WinHTTP transport (`fetch_side`).
#![forbid(unsafe_code)]

pub mod at;
pub mod cache;
pub mod capped;
pub mod choose;
pub mod failure;
pub mod fetch;
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "used only by the Windows WinHTTP backend; tested everywhere"
    )
)]
mod headers;
mod lent;
pub mod problem;
#[cfg(windows)]
mod winhttp_http;

pub use at::{RemoteAt, RemoteAtError};
pub use cache::Cache;
pub use capped::{Budget, Capped, CappedRead, DeadlineGuard, Limits};
pub use choose::{candidates, choose};
pub use failure::problem_of;
pub use fetch::fetch_with;
pub use problem::{HttpStatus, NetworkFailure, RemoteProblem};
pub use sidewise_git_address::{AddressError, RemoteRev, RemoteUrl};
#[cfg(windows)]
pub use winhttp_http::WinHttp;

use sidewise_git::location::GitAt;

/// Fetches the remote side `at` into `cache` through WinHTTP within `Limits::PRODUCT` and returns
/// its stored Git location. Off Windows it fails with `Unsupported`; fetch failures as `RemoteProblem`.
pub fn fetch_side(cache: &Cache, at: &RemoteAt) -> Result<GitAt, RemoteProblem> {
    #[cfg(windows)]
    return cache.fetch_with(at, WinHttp::new(), Limits::PRODUCT);
    #[cfg(not(windows))]
    {
        let _ = (cache, at);
        Err(RemoteProblem::Unsupported)
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn fetch_side_is_unsupported_off_windows() {
        let root = std::env::temp_dir().join("sidewise-fetch-side-never-created");
        let at = RemoteAt {
            url: RemoteUrl::parse("https://example.invalid/repo").unwrap(),
            rev: RemoteRev::parse("main").unwrap(),
            inner: String::new(),
        };
        assert_eq!(
            fetch_side(&Cache::new(root.clone()), &at),
            Err(RemoteProblem::Unsupported)
        );
        assert!(!root.exists(), "no cache folder is created");
    }
}
