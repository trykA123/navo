use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

use super::{Idle, Job, Prefill, lazy_tops};
use crate::shell_menu::fill::{FillAsk, Fills};
use crate::shell_menu::warm::Warm;
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

fn warm_fill(session: u64) -> Warm {
    Warm::Fill {
        session,
        path: vec![0],
    }
}

fn describe(job: Job) -> (u64, Vec<usize>, bool) {
    match job {
        Job::Prefill(prefill) => (prefill.session, prefill.path, true),
        Job::Warm(Warm::Fill { session, path }) => (session, path, false),
        Job::Warm(Warm::Close { session, scratch }) => (session, Vec::new(), scratch.is_some()),
        Job::Warm(Warm::Open { .. }) => panic!("unexpected warm step"),
    }
}

#[test]
fn prefills_of_an_open_menu_run_before_warming() {
    let mut idle = Idle::warming(vec![warm_fill(1)]);
    idle.prefill(prefill(5, &[2]).0);
    let order: Vec<_> = std::iter::from_fn(|| idle.next()).map(describe).collect();
    assert_eq!(order, [(5, vec![2], true), (1, vec![0], false)]);
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
    assert_eq!(left, [(6, vec![1], true)]);
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
    assert_eq!(left, [(5, vec![1], true)]);
}

#[test]
fn warm_steps_added_later_run_after_the_first_ones() {
    let mut idle = Idle::warming(vec![warm_fill(1)]);
    idle.warm_later(Warm::Close {
        session: 1,
        scratch: None,
    });
    let order: Vec<_> = std::iter::from_fn(|| idle.next()).map(describe).collect();
    assert_eq!(order, [(1, vec![0], false), (1, Vec::new(), false)]);
}

#[test]
fn only_lazy_top_level_submenus_are_warmed() {
    let item = |kind| MenuItem {
        kind,
        ..MenuItem::new(
            ItemId::Shell {
                command: 1,
                verb: None,
            },
            "x",
            Group::Other,
        )
    };
    let menu = ShellMenu {
        items: vec![
            item(ItemKind::Command),
            item(ItemKind::Submenu(Submenu::Lazy)),
            item(ItemKind::Submenu(Submenu::Filled(vec![item(
                ItemKind::Submenu(Submenu::Lazy),
            )]))),
            item(ItemKind::Submenu(Submenu::Lazy)),
        ],
    };
    assert_eq!(lazy_tops(&menu), [vec![1], vec![3]]);
}
