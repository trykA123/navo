use std::path::Path;

use rusty_core::retention::{DAY_MS, RetainedState};
use rusty_core::{FsErrorKind, ProviderPath};
use rusty_fs::{FakeFs, FakeOp, Fault, LocalFs, Provider};

use super::*;

mod capture;
mod local;
mod maintenance;
mod owner;
mod purge;
mod restart;

fn open<'a>(profile: &Path, ops: &'a dyn ProviderOps) -> Vault<'a> {
    let (db, _) = Db::open(&profile.join("ferrite.db"), false).unwrap();
    Vault::new(db, profile, ops).unwrap()
}

fn run<R>(action: impl FnOnce(&mut RetentionRun<'_>) -> R) -> R {
    action(&mut RetentionRun::new(&CancelToken::new(), &mut |_| {}))
}

fn capture(vault: &mut Vault<'_>, source: &Location) -> RetentionEntry {
    run(|run| {
        vault.capture(
            CaptureRequest {
                source,
                policy: RetentionPolicy::new(3, 1000, 10).unwrap(),
                created_at: 100,
                job: None,
            },
            run,
        )
    })
    .unwrap()
}

fn restore(
    vault: &mut Vault<'_>,
    id: RetentionId,
    target: &Location,
) -> Result<RetentionEntry, Error> {
    run(|run| vault.restore(RestoreRequest { id, target }, run))
}

fn purge(vault: &mut Vault<'_>, id: RetentionId) -> Result<RetentionEntry, Error> {
    run(|run| vault.purge(id, run))
}

fn reconcile(vault: &mut Vault<'_>, id: RetentionId) -> Result<RetentionEntry, Error> {
    run(|run| vault.reconcile(id, run))
}

fn location(path: &Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

fn raw_state(profile: &Path, id: RetentionId, state: &str) {
    let (db, _) = Db::open(&profile.join("ferrite.db"), false).unwrap();
    db.connection()
        .execute(
            "UPDATE retained_entry SET state = ?2 WHERE id = ?1",
            (id.0, state),
        )
        .unwrap();
}

fn begin_restore(vault: &mut Vault<'_>, id: RetentionId, target: &Location) {
    let mut tx = vault.catalog.transaction().unwrap();
    let mut entry = tx.get(id).unwrap();
    entry.state = RetainedState::Restoring;
    entry.target = Some(rusty_core::retention::RestoreTarget {
        location: target.clone(),
        parent_identity: proof::identity(vault.ops, &target.parent().unwrap()).unwrap(),
    });
    tx.save(&entry, RetainedState::Ready).unwrap();
    tx.commit().unwrap();
}

mod writers;
