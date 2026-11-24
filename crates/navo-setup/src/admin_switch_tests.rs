use std::cell::Cell;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::admin_switch::{Identity, as_admin_with};

/// The marker `Fake::fatal` unwinds with, standing in for the process abort.
#[derive(Debug, PartialEq)]
struct Fatal(String);

/// A thread identity whose revert and enter results are set by the test.
struct Fake {
    entered: bool,
    revert_fails: bool,
    enter_fails: bool,
    enters: Cell<u32>,
}

impl Fake {
    fn new(entered: bool) -> Self {
        Self {
            entered,
            revert_fails: false,
            enter_fails: false,
            enters: Cell::new(0),
        }
    }
}

impl Identity for Fake {
    fn revert(&self) -> io::Result<bool> {
        if self.revert_fails {
            return Err(io::Error::other("revert refused"));
        }
        Ok(self.entered)
    }

    fn enter(&self) -> io::Result<()> {
        self.enters.set(self.enters.get() + 1);
        if self.enter_fails {
            return Err(io::Error::other("enter refused"));
        }
        Ok(())
    }

    fn fatal(&self, error: io::Error) -> ! {
        resume_unwind(Box::new(Fatal(error.to_string())))
    }
}

#[test]
fn entered_thread_runs_f_and_enters_again() {
    let fake = Fake::new(true);
    assert_eq!(as_admin_with(&fake, || 5).ok(), Some(5));
    assert_eq!(fake.enters.get(), 1);
}

#[test]
fn thread_not_entered_stays_on_its_own_token() {
    let fake = Fake::new(false);
    assert_eq!(as_admin_with(&fake, || 5).ok(), Some(5));
    assert_eq!(fake.enters.get(), 0);
}

#[test]
fn failed_revert_is_fatal_and_does_not_run_f() {
    let fake = Fake {
        revert_fails: true,
        ..Fake::new(true)
    };
    let ran = Cell::new(false);
    let outcome = catch_unwind(AssertUnwindSafe(|| as_admin_with(&fake, || ran.set(true))));
    let fatal = outcome
        .err()
        .and_then(|payload| payload.downcast::<Fatal>().ok());
    assert_eq!(
        fatal.map(|boxed| *boxed),
        Some(Fatal("revert refused".to_owned()))
    );
    assert!(!ran.get());
    assert_eq!(fake.enters.get(), 0);
}

#[test]
fn failed_reenter_is_fatal_and_never_returns() {
    let fake = Fake {
        enter_fails: true,
        ..Fake::new(true)
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| as_admin_with(&fake, || 5)));
    let fatal = outcome
        .err()
        .and_then(|payload| payload.downcast::<Fatal>().ok());
    assert_eq!(
        fatal.map(|boxed| *boxed),
        Some(Fatal("enter refused".to_owned()))
    );
    assert_eq!(fake.enters.get(), 1);
}

#[test]
fn panic_in_f_enters_again_before_unwinding_on() {
    let fake = Fake::new(true);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        as_admin_with(&fake, || resume_unwind(Box::new(7u8)))
    }));
    assert!(outcome.is_err());
    assert_eq!(fake.enters.get(), 1);
}
