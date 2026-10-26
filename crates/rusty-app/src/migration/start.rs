use std::io;

use rusty_store::StoreDirs;

use super::{Failure, LenientSync, Migration, Outcome, RealFs, Reason, probe, test_exit_phase};
use crate::profile::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    Blocked,
    InUse,
    Failed { can_start_fresh: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    TryAgain,
    StartFresh,
    Quit,
}

pub(crate) trait Prompt {
    fn ask(&mut self, ask: Ask, failure: Option<&Failure>) -> Choice;
}

#[derive(Debug)]
pub(crate) enum Settled {
    Start(Outcome),
    StartedFresh,
    Quit,
}

#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) settled: Settled,
    sync_failure: Option<io::Error>,
}

pub(crate) fn ask_for(outcome: &Outcome) -> Option<Ask> {
    match outcome {
        Outcome::Blocked => Some(Ask::Blocked),
        Outcome::Failed(failure) if failure.is_in_use() => Some(Ask::InUse),
        Outcome::Failed(failure) => Some(Ask::Failed {
            can_start_fresh: failure.can_start_fresh(),
        }),
        Outcome::Done
        | Outcome::Resumed
        | Outcome::NothingToMigrate(_)
        | Outcome::MigratedRetirePending(_) => None,
    }
}

pub(crate) fn settle(
    mut migrate: impl FnMut() -> Outcome,
    mut start_fresh: impl FnMut() -> Result<(), Failure>,
    prompt: &mut impl Prompt,
) -> Settled {
    let mut outcome = migrate();
    loop {
        let Some(ask) = ask_for(&outcome) else {
            return Settled::Start(outcome);
        };
        let failure = match &outcome {
            Outcome::Failed(failure) => Some(failure),
            _ => None,
        };
        match prompt.ask(ask, failure) {
            Choice::TryAgain => outcome = migrate(),
            Choice::StartFresh
                if matches!(
                    ask,
                    Ask::Failed {
                        can_start_fresh: true
                    }
                ) =>
            {
                match start_fresh() {
                    Ok(()) => return Settled::StartedFresh,
                    Err(failure) => outcome = Outcome::Failed(failure),
                }
            }
            Choice::StartFresh => {}
            Choice::Quit => return Settled::Quit,
        }
    }
}

pub(crate) fn run(profile: &Profile, prompt: &mut impl Prompt) -> Report {
    let Some((old, new)) = roots(profile) else {
        return Report {
            settled: Settled::Start(Outcome::NothingToMigrate(Reason::NoLegacyProfile)),
            sync_failure: None,
        };
    };
    let fs = LenientSync::new(RealFs, cfg!(windows));
    let migration = if old == new {
        Migration::in_profile(&fs, new)
    } else {
        Migration::user(&fs, old.clone(), new)
    }
    .exit_after(test_exit_phase(|name| std::env::var(name).ok()));
    let settled = settle(
        || migration.run(|| probe::legacy(&old) == probe::Legacy::Running),
        || migration.start_fresh(),
        prompt,
    );
    Report {
        settled,
        sync_failure: fs.into_failure(),
    }
}

fn roots(profile: &Profile) -> Option<(StoreDirs, StoreDirs)> {
    match profile {
        Profile::User(Some(new)) => Some((StoreDirs::detect_legacy()?, new.clone())),
        Profile::Folder(dirs) => Some((dirs.clone(), dirs.clone())),
        Profile::User(None) | Profile::Temporary { .. } => None,
    }
}

impl Report {
    pub(crate) fn log(&self) {
        if let Some(error) = &self.sync_failure {
            tracing::warn!(%error, "cannot flush a folder while moving the old profile");
        }
        match &self.settled {
            Settled::Start(Outcome::NothingToMigrate(Reason::NoLegacyProfile)) => {
                tracing::debug!("no old profile to move");
            }
            Settled::Start(Outcome::NothingToMigrate(reason)) => {
                tracing::info!(?reason, "the old profile stays where it is");
            }
            Settled::Start(Outcome::MigratedRetirePending(Some(failure))) => {
                tracing::warn!(
                    ?failure,
                    "the old profile moved; retiring its folders failed"
                );
            }
            settled => tracing::info!(?settled, "old profile check finished"),
        }
    }
}
