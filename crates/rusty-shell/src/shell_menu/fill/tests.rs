use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusty_core::shell_menu::Submenu;

use super::{BACKGROUND_LIMIT, Background, FillAsk, FillReply, Fills, POLL, settle};
use crate::shell_menu::{FILL_BUDGET, MenuError, Pending};

fn fresh(send: impl FnOnce(Vec<usize>, FillReply)) -> impl FnOnce(FillAsk) {
    |ask| match ask {
        FillAsk::Now(path, reply) => send(path, reply),
        FillAsk::Later(..) | FillAsk::Promote(_) => panic!("a plain ask is sent now"),
    }
}

fn ask_and_keep(
    fills: &Fills,
    kept: &Mutex<Vec<FillReply>>,
) -> crate::shell_menu::Pending<Submenu> {
    fills.ask(vec![2], fresh(|_, reply| kept.lock().unwrap().push(reply)))
}

#[test]
fn a_fill_answered_in_time_is_filled() {
    let fills = Fills::default();
    let pending = fills.ask(
        vec![2],
        fresh(|_, reply| reply.send(Ok(Submenu::Filled(Vec::new())))),
    );
    assert_eq!(pending.wait(), Ok(Submenu::Filled(Vec::new())));
}

#[test]
fn a_fill_without_an_answer_is_loading_after_the_budget() {
    let (fills, kept) = (Fills::default(), Mutex::default());
    let started = Instant::now();
    let answer = ask_and_keep(&fills, &kept).wait();
    let elapsed = started.elapsed();
    assert_eq!(answer, Ok(Submenu::Loading));
    assert!(
        elapsed >= FILL_BUDGET && elapsed < FILL_BUDGET * 4,
        "{elapsed:?}"
    );
}

#[test]
fn a_poll_before_the_budget_gives_no_answer_yet() {
    let (fills, kept) = (Fills::default(), Mutex::default());
    assert_eq!(
        ask_and_keep(&fills, &kept).wait_timeout(Duration::ZERO),
        None
    );
}

#[test]
fn asking_again_returns_the_background_result_without_resending() {
    let (fills, kept) = (Fills::default(), Mutex::default());
    assert_eq!(ask_and_keep(&fills, &kept).wait(), Ok(Submenu::Loading));
    kept.lock().unwrap()[0].send(Ok(Submenu::Filled(Vec::new())));
    let again = fills.ask(vec![2], fresh(|_, _| panic!("the fill was sent twice")));
    assert_eq!(
        again.wait_timeout(Duration::ZERO),
        Some(Ok(Submenu::Filled(Vec::new())))
    );
}

#[test]
fn a_dropped_reply_answers_stopped() {
    let fills = Fills::default();
    assert_eq!(
        fills.ask(vec![2], fresh(|_, reply| drop(reply))).wait(),
        Err(MenuError::Stopped)
    );
}

#[test]
fn the_first_answer_wins() {
    let fills = Fills::default();
    let pending = fills.ask(
        vec![2],
        fresh(|_, reply| {
            reply.send(Ok(Submenu::Failed));
            reply.send(Ok(Submenu::Filled(Vec::new())));
        }),
    );
    assert_eq!(pending.wait(), Ok(Submenu::Failed));
}

fn background(fills: &Fills, now: Instant) -> (Vec<Background>, Pending<Submenu>) {
    let mut started = Vec::new();
    let pending = fills.ask(
        vec![2],
        fresh(|path, reply| {
            started.push(Background::new(7, path, reply, now));
        }),
    );
    (started, pending)
}

#[test]
fn a_background_fill_that_fills_is_settled_as_filled() {
    let (fills, now) = (Fills::default(), Instant::now());
    let (mut running, _pending) = background(&fills, now);
    let wait = settle(&mut running, now, |session, path| {
        assert_eq!((session, path), (7, &[2][..]));
        Some(Ok(Submenu::Filled(Vec::new())))
    });
    assert!(running.is_empty());
    assert_eq!(wait, None);
    let answer = fills
        .ask(vec![2], fresh(|_, _| {}))
        .wait_timeout(Duration::ZERO);
    assert_eq!(answer, Some(Ok(Submenu::Filled(Vec::new()))));
}

#[test]
fn a_background_fill_still_loading_keeps_the_loop_polling() {
    let (fills, now) = (Fills::default(), Instant::now());
    let (mut running, _pending) = background(&fills, now);
    let wait = settle(&mut running, now, |_, _| None);
    assert_eq!(running.len(), 1);
    assert_eq!(wait, Some(POLL));
}

#[test]
fn a_background_fill_past_its_limit_fails() {
    let (fills, now) = (Fills::default(), Instant::now());
    let (mut running, pending) = background(&fills, now);
    let wait = settle(&mut running, now + BACKGROUND_LIMIT, |_, _| None);
    assert!(running.is_empty());
    assert_eq!(wait, None);
    let answer = pending.wait_timeout(Duration::ZERO);
    assert_eq!(answer, Some(Ok(Submenu::Failed)));
}

#[test]
fn a_dropped_background_fill_answers_stopped() {
    let (fills, now) = (Fills::default(), Instant::now());
    let (running, pending) = background(&fills, now);
    drop(running);
    let answer = pending.wait_timeout(Duration::ZERO);
    assert_eq!(answer, Some(Err(MenuError::Stopped)));
}

fn poll_until_settled(
    pending: &crate::shell_menu::Pending<Submenu>,
    within: Duration,
) -> Option<Result<Submenu, MenuError>> {
    let stop = Instant::now() + within;
    loop {
        let answer = pending.wait_timeout(Duration::ZERO);
        if !matches!(answer, None | Some(Ok(Submenu::Loading))) || Instant::now() >= stop {
            return answer;
        }
        std::thread::sleep(POLL);
    }
}

#[test]
fn a_fill_the_worker_never_answers_fails_after_the_limit() {
    let (fills, kept) = (
        Fills::with_limit(Duration::from_millis(50)),
        Mutex::default(),
    );
    let pending = ask_and_keep(&fills, &kept);
    assert_eq!(
        pending.wait_timeout(FILL_BUDGET * 4),
        Some(Ok(Submenu::Loading))
    );
    assert_eq!(
        poll_until_settled(&pending, Duration::from_secs(2)),
        Some(Ok(Submenu::Failed))
    );
    assert_eq!(kept.lock().unwrap().len(), 1);
}

#[test]
fn a_late_answer_still_wins_over_the_backstop() {
    let (fills, kept) = (Fills::with_limit(Duration::ZERO), Mutex::default());
    assert_eq!(ask_and_keep(&fills, &kept).wait(), Ok(Submenu::Failed));
    kept.lock().unwrap()[0].send(Ok(Submenu::Filled(Vec::new())));
    let again = fills.ask(vec![2], fresh(|_, _| panic!("the fill was sent twice")));
    assert_eq!(
        again.wait_timeout(Duration::ZERO),
        Some(Ok(Submenu::Filled(Vec::new())))
    );
}

fn ask_again_counting(fills: &Fills, sent: &Mutex<usize>) -> Option<Result<Submenu, MenuError>> {
    let again = fills.ask(
        vec![2],
        fresh(|_, reply| {
            *sent.lock().unwrap() += 1;
            reply.send(Ok(Submenu::Filled(Vec::new())));
        }),
    );
    again.wait_timeout(Duration::ZERO)
}

#[test]
fn asking_again_after_a_failed_fill_sends_it_again() {
    let (fills, sent) = (Fills::default(), Mutex::new(0));
    let first = fills.ask(vec![2], fresh(|_, reply| reply.send(Ok(Submenu::Failed))));
    assert_eq!(first.wait(), Ok(Submenu::Failed));
    let answer = ask_again_counting(&fills, &sent);
    assert_eq!(answer, Some(Ok(Submenu::Filled(Vec::new()))));
    assert_eq!(*sent.lock().unwrap(), 1);
}

#[test]
fn asking_again_after_an_error_sends_it_again() {
    let (fills, sent) = (Fills::default(), Mutex::new(0));
    let first = fills.ask(vec![2], fresh(|_, reply| drop(reply)));
    assert_eq!(first.wait(), Err(MenuError::Stopped));
    let answer = ask_again_counting(&fills, &sent);
    assert_eq!(answer, Some(Ok(Submenu::Filled(Vec::new()))));
    assert_eq!(*sent.lock().unwrap(), 1);
}

#[test]
fn asking_again_after_the_backstop_failed_sends_it_again() {
    let (fills, kept) = (Fills::with_limit(Duration::ZERO), Mutex::default());
    assert_eq!(ask_and_keep(&fills, &kept).wait(), Ok(Submenu::Failed));
    let sent = Mutex::new(0);
    let answer = ask_again_counting(&fills, &sent);
    assert_eq!(answer, Some(Ok(Submenu::Filled(Vec::new()))));
    assert_eq!(*sent.lock().unwrap(), 1);
}

#[test]
fn asking_again_while_loading_does_not_send_it_again() {
    let (fills, kept) = (Fills::default(), Mutex::default());
    assert_eq!(ask_and_keep(&fills, &kept).wait(), Ok(Submenu::Loading));
    let again = fills.ask(vec![2], fresh(|_, _| panic!("the fill was sent twice")));
    assert_eq!(
        again.wait_timeout(Duration::ZERO),
        Some(Ok(Submenu::Loading))
    );
}

#[derive(Debug, PartialEq)]
enum Sent {
    Now,
    Later,
    Promote(Vec<usize>),
}

fn record(sent: &Mutex<Vec<Sent>>, kept: &Mutex<Vec<FillReply>>) -> impl FnOnce(FillAsk) {
    |ask| {
        let (kind, reply) = match ask {
            FillAsk::Now(_, reply) => (Sent::Now, Some(reply)),
            FillAsk::Later(_, reply) => (Sent::Later, Some(reply)),
            FillAsk::Promote(path) => (Sent::Promote(path), None),
        };
        sent.lock().unwrap().push(kind);
        kept.lock().unwrap().extend(reply);
    }
}

#[test]
fn a_prefill_is_sent_for_later() {
    let (fills, sent, kept) = (Fills::default(), Mutex::default(), Mutex::default());
    let _prefill = fills.prefill(vec![2], record(&sent, &kept));
    assert_eq!(*sent.lock().unwrap(), [Sent::Later]);
}

#[test]
fn asking_for_a_waiting_prefill_promotes_it_once_and_shares_its_answer() {
    let (fills, sent, kept) = (Fills::default(), Mutex::default(), Mutex::default());
    let _prefill = fills.prefill(vec![2], record(&sent, &kept));
    let asked = fills.ask(vec![2], record(&sent, &kept));
    let again = fills.ask(vec![2], record(&sent, &kept));
    assert_eq!(*sent.lock().unwrap(), [Sent::Later, Sent::Promote(vec![2])]);
    kept.lock().unwrap()[0].send(Ok(Submenu::Filled(Vec::new())));
    assert_eq!(asked.wait(), Ok(Submenu::Filled(Vec::new())));
    assert_eq!(again.wait(), Ok(Submenu::Filled(Vec::new())));
}

#[test]
fn asking_for_a_finished_prefill_answers_at_once_without_sending() {
    let (fills, sent, kept) = (Fills::default(), Mutex::default(), Mutex::default());
    let _prefill = fills.prefill(vec![2], record(&sent, &kept));
    kept.lock().unwrap()[0].send(Ok(Submenu::Filled(Vec::new())));
    let asked = fills.ask(vec![2], record(&sent, &kept));
    assert_eq!(*sent.lock().unwrap(), [Sent::Later]);
    let answer = asked.wait_timeout(Duration::ZERO);
    assert_eq!(answer, Some(Ok(Submenu::Filled(Vec::new()))));
}

#[test]
fn a_prefill_after_a_plain_ask_sends_nothing() {
    let (fills, sent, kept) = (Fills::default(), Mutex::default(), Mutex::default());
    let _asked = fills.ask(vec![2], record(&sent, &kept));
    let _prefill = fills.prefill(vec![2], record(&sent, &kept));
    assert_eq!(*sent.lock().unwrap(), [Sent::Now]);
}

#[test]
fn a_prefill_waiting_in_the_idle_queue_never_gives_up() {
    let (fills, sent, kept) = (
        Fills::with_limit(Duration::ZERO),
        Mutex::default(),
        Mutex::default(),
    );
    let prefill = fills.prefill(vec![2], record(&sent, &kept));
    assert_eq!(
        prefill.wait_timeout(FILL_BUDGET * 2),
        Some(Ok(Submenu::Loading))
    );
    let asked = fills.ask(vec![2], record(&sent, &kept));
    assert_eq!(*sent.lock().unwrap(), [Sent::Later, Sent::Promote(vec![2])]);
    assert_eq!(
        asked.wait_timeout(FILL_BUDGET * 2),
        Some(Ok(Submenu::Failed))
    );
}
