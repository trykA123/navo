use rusty_core::Location;
use rusty_core::retention::{CopyTemporary, RetainedWritePlan};
use rusty_fs::ProviderOps;
use rusty_store::UnfinishedJob;

use super::{SharedJournal, clear, decode, scope};

pub(crate) fn cleanup(
    journal: &SharedJournal,
    (job, plan): (&UnfinishedJob, &RetainedWritePlan),
    ops: &dyn ProviderOps,
) -> Result<(), String> {
    let evidence = job
        .steps
        .iter()
        .filter_map(|step| step.detail.as_ref().map(|bytes| (step.seq, bytes)))
        .map(|(seq, bytes)| decode(bytes, (seq, plan)).map(|proof| (seq, bytes, proof)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut failed = None;
    for (seq, bytes, proof) in evidence {
        if let Err(error) = remove(ops, &proof).and_then(|()| clear(journal, job.id, (seq, bytes)))
        {
            failed.get_or_insert(error);
        }
    }
    failed.map_or(Ok(()), Err)
}

fn remove(ops: &dyn ProviderOps, proof: &CopyTemporary) -> Result<(), String> {
    scope::parent(ops, proof)?;
    if scope::identity(ops, &proof.temporary)?.is_none() {
        return Ok(());
    }
    scope::present(ops, proof)?;
    let identity = proof.identity.ok_or("copy temporary was never proved")?;
    ops.remove_owned_file(&proof.temporary, identity)
        .map_err(|error| error.message())?;
    scope::absent(ops, proof)
}

pub(crate) fn paths(job: &UnfinishedJob) -> Vec<Location> {
    job.steps
        .iter()
        .filter_map(|step| step.detail.as_ref())
        .filter_map(|bytes| serde_json::from_slice::<CopyTemporary>(bytes).ok())
        .map(|proof| proof.temporary)
        .collect()
}
