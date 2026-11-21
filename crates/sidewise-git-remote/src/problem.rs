//! Why a remote fetch failed: the problem kinds, the status table and their texts.

use std::fmt;

use sidewise_engine::model::Reason;

/// The reason a remote Git side could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteProblem {
    Unreachable,
    NotFound,
    Redirected,
    NoSuchRevision,
    TooLarge,
    ObjectTooLarge,
    TimedOut,
    Broken,
    Unsupported,
}

/// An HTTP status a backend reports as an error (wrapped in `io::Error::other`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpStatus(pub u16);

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP status {}", self.0)
    }
}

impl std::error::Error for HttpStatus {}

/// A system network error code a backend reports (wrapped in `io::Error::other`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkFailure(pub u32);

impl fmt::Display for NetworkFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "network error {}", self.0)
    }
}

impl std::error::Error for NetworkFailure {}

impl RemoteProblem {
    /// Maps a non-2xx HTTP status to a problem: 401, 403, 404 not found; 3xx redirected;
    /// 407 unreachable; anything else broken.
    pub fn from_status(status: u16) -> Self {
        match status {
            401 | 403 | 404 => Self::NotFound,
            300..=399 => Self::Redirected,
            407 => Self::Unreachable,
            _ => Self::Broken,
        }
    }

    /// The l10n id of this problem's text; `Unsupported` shares the text of `Broken`.
    pub const fn fluent_id(self) -> &'static str {
        match self {
            Self::Unreachable => "git-remote-unreachable",
            Self::NotFound => "git-remote-not-found",
            Self::Redirected => "git-remote-redirected",
            Self::NoSuchRevision => "git-remote-no-such-revision",
            Self::TooLarge => "git-remote-too-large",
            Self::ObjectTooLarge => "git-remote-object-too-large",
            Self::TimedOut => "git-remote-timed-out",
            Self::Broken | Self::Unsupported => "git-remote-broken",
        }
    }

    /// The engine reason shown on a row when this problem makes a side unavailable.
    pub const fn reason(self) -> Reason {
        match self {
            Self::NotFound | Self::Redirected | Self::NoSuchRevision => Reason::NotFound,
            Self::TimedOut => Reason::Stopped,
            Self::Unsupported => Reason::Unsupported,
            Self::Unreachable | Self::TooLarge | Self::ObjectTooLarge | Self::Broken => {
                Reason::ReadError
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_table() {
        for s in [401, 403, 404] {
            assert_eq!(RemoteProblem::from_status(s), RemoteProblem::NotFound);
        }
        for s in [300, 301, 302, 399] {
            assert_eq!(RemoteProblem::from_status(s), RemoteProblem::Redirected);
        }
        assert_eq!(RemoteProblem::from_status(407), RemoteProblem::Unreachable);
        for s in [400, 400 + 10, 500, 503, 200, 100] {
            assert_eq!(RemoteProblem::from_status(s), RemoteProblem::Broken);
        }
    }

    #[test]
    fn ids_are_distinct_except_unsupported() {
        use RemoteProblem::*;
        let all = [
            Unreachable,
            NotFound,
            Redirected,
            NoSuchRevision,
            TooLarge,
            ObjectTooLarge,
            TimedOut,
            Broken,
        ];
        let ids: std::collections::HashSet<_> = all.iter().map(|p| p.fluent_id()).collect();
        assert_eq!(ids.len(), all.len());
        assert_eq!(Unsupported.fluent_id(), Broken.fluent_id());
    }

    #[test]
    fn reasons() {
        assert_eq!(RemoteProblem::NotFound.reason(), Reason::NotFound);
        assert_eq!(RemoteProblem::NoSuchRevision.reason(), Reason::NotFound);
        assert_eq!(RemoteProblem::TimedOut.reason(), Reason::Stopped);
        assert_eq!(RemoteProblem::Unsupported.reason(), Reason::Unsupported);
        assert_eq!(RemoteProblem::Unreachable.reason(), Reason::ReadError);
    }
}
