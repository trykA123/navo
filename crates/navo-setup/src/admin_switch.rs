//! The `as_admin` switch, kept off the Windows calls so its failure paths test on any host: run on
//! Setup's own token, then act as the desktop user again (IW39).

use std::io;

/// The thread identity calls `as_admin` makes; `probe::shell` is the Windows one.
pub(crate) trait Identity {
    /// Drops this thread's impersonation; returns whether the thread was acting as the user.
    fn revert(&self) -> io::Result<bool>;
    /// Acts as the desktop user again.
    fn enter(&self) -> io::Result<()>;
    /// Ends the process, since this thread would otherwise go on with Setup's admin token.
    fn fatal(&self, error: io::Error) -> !;
}

/// Runs `f` on Setup's own token, then acts as the user again if the thread was acting as the user.
/// A failed revert (before `f`, which then never runs) or a failed re-entry (after `f` or while `f`
/// panics) calls `fatal` and never returns: the thread's token would be wrong or unknown.
pub(crate) fn as_admin_with<T>(identity: &impl Identity, f: impl FnOnce() -> T) -> io::Result<T> {
    let armed = match identity.revert() {
        Ok(armed) => armed,
        Err(error) => identity.fatal(error),
    };
    let _reenter = Reenter { identity, armed };
    Ok(f())
}

/// Acts as the user again when it drops, after `f` returns or while `f` unwinds.
struct Reenter<'a, I: Identity> {
    identity: &'a I,
    armed: bool,
}

impl<I: Identity> Drop for Reenter<'_, I> {
    /// Enters again when armed; a failure ends the process through `fatal`.
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(error) = self.identity.enter() {
            self.identity.fatal(error);
        }
    }
}
