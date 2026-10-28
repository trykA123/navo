use rusty_core::shell_menu::Submenu;

use super::{Idle, Prefill};
use crate::shell_menu::fill::{FillAsk, Fills};
use crate::shell_menu::{MenuError, Pending};

fn prefill(session: u64, path: &[usize]) -> (Prefill, Pending<Submenu>) {
    let mut sent = None;
    let pending = Fills::default().prefill(path.to_vec(), |ask| {
        if let FillAsk::Later(path, reply) = ask {
            sent = Some(Prefill {
                session,
                path,
                reply,
            });
        }
    });
    (sent.expect("a prefill is sent for later"), pending)
}

fn describe(prefill: Prefill) -> (u64, Vec<usize>) {
    (prefill.session, prefill.path)
}

#[test]
fn a_new_idle_queue_has_no_work_so_nothing_loads_before_the_first_menu() {
    let mut idle = Idle::default();
    assert!(idle.next().is_none());
}

#[test]
fn prefills_of_an_open_menu_run_in_the_order_they_came() {
    let mut idle = Idle::default();
    idle.prefill(prefill(5, &[2]).0);
    idle.prefill(prefill(5, &[1]).0);
    let order: Vec<_> = std::iter::from_fn(|| idle.next()).map(describe).collect();
    assert_eq!(order, [(5, vec![2]), (5, vec![1])]);
}

#[test]
fn closing_a_menu_drops_its_waiting_prefills_and_answers_them_stopped() {
    let mut idle = Idle::default();
    let (closed, answer) = prefill(5, &[2]);
    idle.prefill(closed);
    idle.prefill(prefill(6, &[1]).0);
    idle.close(5);
    assert_eq!(answer.wait(), Err(MenuError::Stopped));
    let left: Vec<_> = std::iter::from_fn(|| idle.next()).map(describe).collect();
    assert_eq!(left, [(6, vec![1])]);
}

#[test]
fn promoting_takes_only_the_matching_prefill_out_of_the_queue() {
    let mut idle = Idle::default();
    idle.prefill(prefill(5, &[1]).0);
    idle.prefill(prefill(5, &[2]).0);
    let promoted = idle.promote(5, &[2]).expect("the waiting prefill");
    assert_eq!((promoted.session, promoted.path.as_slice()), (5, &[2][..]));
    promoted.reply.send(Ok(Submenu::Failed));
    assert!(idle.promote(5, &[2]).is_none());
    assert!(idle.promote(6, &[1]).is_none());
    let left: Vec<_> = std::iter::from_fn(|| idle.next()).map(describe).collect();
    assert_eq!(left, [(5, vec![1])]);
}
