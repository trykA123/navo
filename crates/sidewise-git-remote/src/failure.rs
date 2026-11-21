//! Maps a failed gix fetch (`gix::Error`, i.e. `gix_error::Error`) to the `RemoteProblem` the user
//! sees, in the fixed order of the plan: budget slot, sign-in, transport, object size, the rest.

use std::error::Error;
use std::io;

use gix::error::{Class, ResourceExhaustionKind};
use gix_transport::client::AuthenticationRequired;

use crate::capped::Budget;
use crate::{HttpStatus, NetworkFailure, RemoteProblem};

/// The WinHTTP error code for a timed-out request (`ERROR_WINHTTP_TIMEOUT`).
const WINHTTP_TIMEOUT: u32 = 12002;

/// The problem behind `err`: the cap recorded in `budget` first, then the error chain; never fails.
pub fn problem_of(err: &gix::Error, budget: &Budget) -> RemoteProblem {
    if let Some(problem) = budget.problem() {
        return problem;
    }
    // A refused sign-in loses the 401 cause in gix and keeps only the `Unauthenticated` class.
    if err.is_unauthenticated() || err.iter_errors().any(|e| e.is::<AuthenticationRequired>()) {
        return RemoteProblem::NotFound;
    }
    if let Some(problem) = err.iter_errors().find_map(transport_problem) {
        return problem;
    }
    let alloc = Class::ResourceExhaustion(ResourceExhaustionKind::AllocationLimit);
    if err.classify().has(alloc) {
        return RemoteProblem::ObjectTooLarge;
    }
    RemoteProblem::Broken
}

/// The problem a backend reported as an `io::Error` wrapping `HttpStatus` or `NetworkFailure`.
fn transport_problem(err: &(dyn Error + 'static)) -> Option<RemoteProblem> {
    let inner = err.downcast_ref::<io::Error>()?.get_ref()?;
    if let Some(status) = inner.downcast_ref::<HttpStatus>() {
        return Some(RemoteProblem::from_status(status.0));
    }
    inner
        .downcast_ref::<NetworkFailure>()
        .map(|failure| match failure.0 {
            WINHTTP_TIMEOUT => RemoteProblem::TimedOut,
            _ => RemoteProblem::Unreachable,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capped::Limits;

    /// A gix error as a fetch returns it: `cause` raised under one outer context message.
    fn wrapped(cause: impl Error + Send + Sync + 'static) -> gix::Error {
        gix::Exn::new(cause)
            .raise(gix::error::message("fetch failed"))
            .into_error()
    }

    fn fresh() -> Budget {
        Budget::new(Limits::PRODUCT)
    }

    fn io_of(inner: impl Error + Send + Sync + 'static) -> io::Error {
        io::Error::other(inner)
    }

    #[test]
    fn slot_comes_first() {
        let budget = Budget::new(Limits {
            max_bytes: 0,
            deadline: Limits::PRODUCT.deadline,
        });
        let _ = budget.charge(1);
        let err = wrapped(io::Error::new(
            io::ErrorKind::PermissionDenied,
            AuthenticationRequired::default(),
        ));
        assert_eq!(problem_of(&err, &budget), RemoteProblem::TooLarge);
    }

    #[test]
    fn sign_in_is_not_found() {
        let err = wrapped(io::Error::new(
            io::ErrorKind::PermissionDenied,
            AuthenticationRequired::default(),
        ));
        assert_eq!(problem_of(&err, &fresh()), RemoteProblem::NotFound);
    }

    #[test]
    fn refused_sign_in_is_not_found() {
        // With credentials refused, gix drops the 401 cause and keeps only this class.
        let cause: gix::Error = gix::error::unauthenticated("no credentials returned").into();
        let err = gix::Exn::new(cause)
            .raise(gix::error::message("fetch failed"))
            .into_error();
        assert_eq!(problem_of(&err, &fresh()), RemoteProblem::NotFound);
    }

    #[test]
    fn statuses_map_through_the_table() {
        for (status, want) in [
            (404, RemoteProblem::NotFound),
            (302, RemoteProblem::Redirected),
            (407, RemoteProblem::Unreachable),
            (500, RemoteProblem::Broken),
        ] {
            let err = wrapped(io_of(HttpStatus(status)));
            assert_eq!(problem_of(&err, &fresh()), want, "status {status}");
        }
    }

    #[test]
    fn network_failures() {
        let timeout = wrapped(io_of(NetworkFailure(12002)));
        assert_eq!(problem_of(&timeout, &fresh()), RemoteProblem::TimedOut);
        let refused = wrapped(io_of(NetworkFailure(12029)));
        assert_eq!(problem_of(&refused, &fresh()), RemoteProblem::Unreachable);
    }

    #[test]
    fn allocation_limit_is_object_too_large() {
        let cause: gix::Error = gix::error::allocation_limit("delta too big").into();
        let err = gix::Exn::new(cause)
            .raise(gix::error::message("fetch failed"))
            .into_error();
        assert_eq!(problem_of(&err, &fresh()), RemoteProblem::ObjectTooLarge);
    }

    #[test]
    fn anything_else_is_broken() {
        let err = wrapped(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "pack cut short",
        ));
        assert_eq!(problem_of(&err, &fresh()), RemoteProblem::Broken);
        let plain: gix::Error = gix::error::message("bad pack").into();
        assert_eq!(problem_of(&plain, &fresh()), RemoteProblem::Broken);
    }
}
