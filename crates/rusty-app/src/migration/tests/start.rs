use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;

use super::super::start::{Ask, Choice, Prompt, Settled, ask_for, settle};
use super::super::{Failure, Outcome, Reason, Step};

#[derive(Default)]
struct Script {
    choices: VecDeque<Choice>,
    asked: Vec<Ask>,
}

impl Prompt for Script {
    fn ask(&mut self, ask: Ask, _: Option<&Failure>) -> Choice {
        self.asked.push(ask);
        self.choices
            .pop_front()
            .expect("the test scripted every answer")
    }
}

fn script(choices: &[Choice]) -> Script {
    Script {
        choices: choices.iter().copied().collect(),
        asked: Vec::new(),
    }
}

fn failed(committed: bool) -> Outcome {
    Outcome::Failed(Failure::new(
        Step::Stage,
        io::Error::other("disk full"),
        committed,
    ))
}

fn outcomes(list: Vec<Outcome>) -> impl FnMut() -> Outcome {
    let list = RefCell::new(VecDeque::from(list));
    move || {
        list.borrow_mut()
            .pop_front()
            .expect("no more migration runs")
    }
}

#[test]
fn outcomes_that_start_navo_never_ask() {
    let starting = [
        Outcome::Done,
        Outcome::Resumed,
        Outcome::NothingToMigrate(Reason::NoLegacyProfile),
        Outcome::NothingToMigrate(Reason::NavoProfileExists),
        Outcome::NothingToMigrate(Reason::AlreadyMigrated),
        Outcome::NothingToMigrate(Reason::StartedFresh),
        Outcome::MigratedRetirePending(None),
        Outcome::MigratedRetirePending(Some(Failure::new(
            Step::Retire,
            io::Error::other("busy"),
            true,
        ))),
    ];
    for outcome in starting {
        let mut prompt = script(&[]);
        let label = format!("{outcome:?}");
        let mut once = Some(outcome);
        let settled = settle(|| once.take().unwrap(), || unreachable!(), &mut prompt);
        assert!(matches!(settled, Settled::Start(_)), "{label}");
        assert!(prompt.asked.is_empty(), "{label}");
    }
}

#[test]
fn blocked_and_failures_ask_the_right_question() {
    assert_eq!(ask_for(&Outcome::Blocked), Some(Ask::Blocked));
    assert_eq!(
        ask_for(&failed(false)),
        Some(Ask::Failed {
            can_start_fresh: true
        })
    );
    assert_eq!(
        ask_for(&failed(true)),
        Some(Ask::Failed {
            can_start_fresh: false
        })
    );
}

#[cfg(windows)]
#[test]
fn a_file_in_use_without_a_running_ferrite_asks_a_generic_question() {
    for (step, code) in [(Step::Stage, 32), (Step::Commit, 5)] {
        let error = io::Error::from_raw_os_error(code);
        let outcome = Outcome::Failed(Failure::new(step, error, false));
        assert_eq!(ask_for(&outcome), Some(Ask::InUse), "{step:?}");
    }
}

#[test]
fn blocked_then_try_again_migrates_once_ferrite_is_closed() {
    let mut prompt = script(&[Choice::TryAgain]);
    let migrate = outcomes(vec![Outcome::Blocked, Outcome::Done]);
    let settled = settle(migrate, || unreachable!(), &mut prompt);
    assert!(matches!(settled, Settled::Start(Outcome::Done)));
    assert_eq!(prompt.asked, [Ask::Blocked]);
}

#[test]
fn blocked_then_quit_does_not_start() {
    let mut prompt = script(&[Choice::Quit]);
    let settled = settle(
        outcomes(vec![Outcome::Blocked]),
        || unreachable!(),
        &mut prompt,
    );
    assert!(matches!(settled, Settled::Quit));
}

#[test]
fn start_fresh_after_a_failure_starts_navo() {
    let mut prompt = script(&[Choice::StartFresh]);
    let mut fresh = 0;
    let settled = settle(
        outcomes(vec![failed(false)]),
        || {
            fresh += 1;
            Ok(())
        },
        &mut prompt,
    );
    assert!(matches!(settled, Settled::StartedFresh));
    assert_eq!(fresh, 1);
}

#[test]
fn start_fresh_is_refused_after_the_commit() {
    let mut prompt = script(&[Choice::StartFresh, Choice::Quit]);
    let settled = settle(outcomes(vec![failed(true)]), || unreachable!(), &mut prompt);
    assert!(matches!(settled, Settled::Quit));
    let committed = Ask::Failed {
        can_start_fresh: false,
    };
    assert_eq!(prompt.asked, [committed, committed]);
}

#[test]
fn a_failed_start_fresh_asks_again() {
    let mut prompt = script(&[Choice::StartFresh, Choice::TryAgain]);
    let settled = settle(
        outcomes(vec![failed(false), Outcome::Resumed]),
        || {
            let error = io::Error::other("read-only");
            Err(Failure::new(Step::Finish, error, false))
        },
        &mut prompt,
    );
    assert!(matches!(settled, Settled::Start(Outcome::Resumed)));
    assert_eq!(prompt.asked.len(), 2);
}

#[test]
fn the_dialog_offers_start_fresh_only_before_the_commit() {
    use super::super::dialog::Text;
    use Choice::{Quit, StartFresh, TryAgain};

    let offered = |ask| -> Vec<Choice> {
        Text::new(ask, None)
            .choices
            .into_iter()
            .map(|(choice, _)| choice)
            .collect()
    };
    let fresh = Ask::Failed {
        can_start_fresh: true,
    };
    let committed = Ask::Failed {
        can_start_fresh: false,
    };
    assert_eq!(offered(fresh), [TryAgain, StartFresh, Quit]);
    assert_eq!(offered(committed), [TryAgain, Quit]);
    assert_eq!(offered(Ask::Blocked), [TryAgain, Quit]);
    assert_eq!(offered(Ask::InUse), [TryAgain, Quit]);
    let in_use = Text::new(Ask::InUse, None);
    assert_ne!(in_use.heading, Text::new(Ask::Blocked, None).heading);
}
