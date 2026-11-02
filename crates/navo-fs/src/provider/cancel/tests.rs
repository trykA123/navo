use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use super::CancelToken;

const WAIT: Duration = Duration::from_secs(5);
const HOLD: Duration = Duration::from_millis(50);

fn waiter(token: &CancelToken) -> Receiver<bool> {
    let (sender, woke) = channel();
    let token = token.clone();
    thread::spawn(move || {
        token.wait_while_paused();
        let _ = sender.send(token.is_cancelled());
    });
    woke
}

fn paused() -> CancelToken {
    let token = CancelToken::new();
    token.pause();
    token
}

#[test]
fn a_new_token_is_neither_paused_nor_cancelled() {
    let token = CancelToken::new();
    assert_eq!((token.is_paused(), token.is_cancelled()), (false, false));
}

#[test]
fn pause_and_resume_flip_the_paused_state_for_every_clone() {
    let token = CancelToken::new();
    let clone = token.clone();
    token.pause();
    assert!(clone.is_paused());
    clone.resume();
    assert!(!token.is_paused());
}

#[test]
fn an_unpaused_token_does_not_wait() {
    assert_eq!(waiter(&CancelToken::new()).recv_timeout(WAIT), Ok(false));
}

#[test]
fn a_paused_wait_holds_until_resumed() {
    let token = paused();
    let woke = waiter(&token);
    assert_eq!(woke.recv_timeout(HOLD), Err(RecvTimeoutError::Timeout));
    token.resume();
    assert_eq!(woke.recv_timeout(WAIT), Ok(false));
}

#[test]
fn cancel_wakes_a_paused_wait_at_once() {
    let token = paused();
    let woke = waiter(&token);
    token.cancel();
    assert_eq!(woke.recv_timeout(WAIT), Ok(true));
}

#[test]
fn a_cancelled_token_never_waits_even_when_paused() {
    let token = paused();
    token.cancel();
    assert_eq!(waiter(&token).recv_timeout(WAIT), Ok(true));
}
