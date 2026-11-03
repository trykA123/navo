//! Update core: the feed trait and the version rules. No network, no installer.

#[cfg(any(test, feature = "test-support"))]
pub mod fake;
mod machine;
mod runner;
mod version;

pub use machine::{Effect, Event, UpdateState, step};
pub use runner::{CHECK_TIMEOUT, UpdateRunner};
pub use semver::Version;
pub use version::{offer, parse_version};

/// How this copy of Navo was installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    Installed,
    Portable,
    Unpackaged,
}

/// A release a feed can offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub size: u64,
}

/// Why a feed call failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    #[error("offline")]
    Offline,
    #[error("rate limited")]
    RateLimited,
    #[error("bad feed")]
    BadFeed,
    #[error("download interrupted")]
    Interrupted,
    #[error("corrupt download")]
    Corrupt,
    #[error("another download is running")]
    Busy,
    #[error("{0}")]
    Other(String),
}

/// The answer of a check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Nothing,
    Release(Release),
}

/// Whether a found release is worth downloading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    Newer(Release),
    NotNewer,
}

/// Source of updates. Calls block; the caller runs them off the UI thread.
pub trait UpdateFeed: Send + Sync {
    fn kind(&self) -> InstallKind;
    fn current(&self) -> Version;
    fn staged(&self) -> Option<Version>;
    fn check(&self) -> Result<Found, FeedError>;
    /// Reports whole percent, 0 to 100.
    fn download(&self, release: &Release, progress: &dyn Fn(u8)) -> Result<(), FeedError>;
}
