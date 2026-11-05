use std::sync::{Arc, Condvar, Mutex as StdMutex, mpsc};
use std::time::Instant;

use super::*;

const WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum State {
    #[default]
    Free,
    Held,
    Abandoned,
}

#[derive(Clone, Default)]
struct FakeMutex {
    state: Arc<(StdMutex<State>, Condvar)>,
    waiting: Option<mpsc::Sender<()>>,
}

impl Mutex for FakeMutex {
    fn wait(&self, wait: Duration) -> io::Result<WaitResult> {
        let (state, changed) = &*self.state;
        let mut state = state.lock().unwrap();
        let deadline = Instant::now() + wait;
        if *state == State::Held
            && let Some(waiting) = &self.waiting
        {
            waiting.send(()).unwrap();
        }
        while *state == State::Held {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(WaitResult::TimedOut);
            };
            state = changed.wait_timeout(state, remaining).unwrap().0;
        }
        let result = match *state {
            State::Abandoned => WaitResult::Abandoned,
            _ => WaitResult::Acquired,
        };
        *state = State::Held;
        Ok(result)
    }

    fn release(&self) -> io::Result<()> {
        let (state, changed) = &*self.state;
        *state.lock().unwrap() = State::Free;
        changed.notify_one();
        Ok(())
    }
}

fn try_from_another_thread(mutex: FakeMutex) -> bool {
    std::thread::spawn(move || Held::acquire(mutex, Duration::ZERO).unwrap().is_some())
        .join()
        .unwrap()
}

#[test]
fn a_second_acquire_waits_then_gets_the_lock_on_drop() {
    let mutex = FakeMutex::default();
    let first = Held::acquire(mutex.clone(), WAIT).unwrap().unwrap();
    let (waiting, observed) = mpsc::channel();
    let (acquired, finished) = mpsc::channel();
    let second = FakeMutex {
        waiting: Some(waiting),
        ..mutex
    };
    let worker = std::thread::spawn(move || {
        let held = Held::acquire(second, WAIT).unwrap().unwrap();
        acquired.send(()).unwrap();
        drop(held);
    });
    observed.recv_timeout(WAIT).unwrap();
    assert_eq!(finished.try_recv(), Err(mpsc::TryRecvError::Empty));
    drop(first);
    finished.recv_timeout(WAIT).unwrap();
    worker.join().unwrap();
}

#[test]
fn a_try_acquire_returns_none_while_held() {
    let mutex = FakeMutex::default();
    let held = Held::acquire(mutex.clone(), WAIT).unwrap().unwrap();
    assert!(!try_from_another_thread(mutex.clone()));
    drop(held);
    assert!(Held::acquire(mutex, Duration::ZERO).unwrap().is_some());
}

#[test]
fn an_abandoned_mutex_is_acquired_and_released() {
    let mutex = FakeMutex::default();
    *mutex.state.0.lock().unwrap() = State::Abandoned;
    let held = Held::acquire(mutex.clone(), WAIT).unwrap();
    assert!(
        held.is_some(),
        "the OS transfers ownership after abandonment"
    );
    assert!(!try_from_another_thread(mutex.clone()));
    drop(held);
    assert!(Held::acquire(mutex, Duration::ZERO).unwrap().is_some());
}

#[test]
fn a_wait_timeout_does_not_release_someone_elses_lock() {
    let mutex = FakeMutex::default();
    let _held = Held::acquire(mutex.clone(), WAIT).unwrap().unwrap();
    assert!(!try_from_another_thread(mutex.clone()));
    assert!(!try_from_another_thread(mutex));
}

#[test]
fn scopes_name_the_same_change_in_their_own_namespace() {
    assert_eq!(Scope::Global.name(), "Global\\NavoIndexServiceChange");
    assert_eq!(Scope::Local.name(), "Local\\NavoIndexServiceChange");
}

#[derive(Default)]
struct FakeScm {
    current: bool,
    changes: Vec<&'static str>,
}

impl FakeScm {
    fn install(&mut self) -> io::Result<()> {
        let current = self.current;
        install_unless_current(
            || Ok(current),
            || {
                self.changes.extend(["copy", "stop", "configure", "start"]);
                self.current = true;
                Ok(())
            },
        )
    }
}

#[test]
fn an_already_current_install_changes_nothing() {
    let mut scm = FakeScm {
        current: true,
        ..FakeScm::default()
    };
    scm.install().unwrap();
    assert!(scm.changes.is_empty());
}

#[test]
fn an_install_rechecks_after_the_previous_change() {
    let mut scm = FakeScm::default();
    scm.install().unwrap();
    let changes = scm.changes.clone();
    assert!(!changes.is_empty());
    scm.install().unwrap();
    assert_eq!(scm.changes, changes);
}

#[test]
fn a_failed_current_check_does_not_change_the_service() {
    let result = install_unless_current(
        || Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        || unreachable!("a failed check must leave the service untouched"),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}
