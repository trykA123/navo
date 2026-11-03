use std::collections::HashSet;

use navo_core::retention::{
    MAX_NODES, PurgePlan, PurgeRequest, RetainedState, RetentionId, RetentionPolicy,
    RetentionSummary, VolumeSpace, select_purge_inventory,
};
use navo_core::{FileIdentity, Location};

use crate::{Error, RetentionRun, Vault, proof};

pub struct MaintenanceRequest<'a> {
    pub at: &'a Location,
    pub policy: RetentionPolicy,
    pub now: i64,
    pub max_purges: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    pub purged: Vec<RetentionId>,
    pub skipped: Vec<RetentionId>,
    pub remaining_bytes: u128,
    pub quota_unmet: bool,
    pub low_space: bool,
    pub budget_exhausted: bool,
    pub cleanup_pending: bool,
}

struct Observation<'a> {
    at: &'a Location,
    identity: FileIdentity,
    mount: u64,
}

impl Vault<'_> {
    pub fn maintain_volume(
        &mut self,
        request: &MaintenanceRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<MaintenanceReport, Error> {
        self.maintain_inner(request, run, &HashSet::new())
    }

    pub fn maintain_job_volume(
        &mut self,
        request: &MaintenanceRequest<'_>,
        job: navo_store::JobId,
        run: &mut RetentionRun<'_>,
    ) -> Result<MaintenanceReport, Error> {
        let protected = self.catalog.ids_for_job(job)?.into_iter().collect();
        self.maintain_inner(request, run, &protected)
    }

    fn maintain_inner(
        &mut self,
        request: &MaintenanceRequest<'_>,
        run: &mut RetentionRun<'_>,
        protected: &HashSet<RetentionId>,
    ) -> Result<MaintenanceReport, Error> {
        let observed = self.observe(request, run)?;
        let mut inventory = self.inventory((request.at, run), protected)?;
        let (mut skipped, mut attempts) =
            self.cleanup_terminal(&observed, (request.max_purges, run))?;
        let mut purged = Vec::new();
        loop {
            let space = observed.measure(self.ops)?;
            let plan = select_purge_inventory(
                &inventory,
                request.policy,
                PurgeRequest {
                    space,
                    now: request.now,
                },
            )?;
            let next = next_attempt(&inventory, &plan, space.volume);
            let Some(next) = next.filter(|_| attempts < request.max_purges) else {
                let mut result = report(purged, &inventory, (request, space, next.is_some()));
                result.skipped = skipped;
                result.cleanup_pending = !self.catalog.pending_cleanup(space.volume, 1)?.is_empty();
                return Ok(result);
            };
            attempts += 1;
            self.catalog.begin_maintenance_attempt(next.id())?;
            let attempt = self.expire(next.id(), next.resumes(space.volume), run);
            if attempt.as_ref().is_err_and(|error| cancelled(error, run)) {
                self.catalog.cancel_maintenance_attempt(next.id())?;
            }
            match attempt? {
                Attempt::Purged => {
                    purged.push(next.id());
                    inventory.retain(|entry| entry.id() != next.id());
                }
                Attempt::Settled => self.refresh(&mut inventory, next)?,
                Attempt::Skipped => {
                    skipped.push(next.id());
                    exclude_from_selection(&mut inventory, next.id());
                }
            }
        }
    }

    fn observe<'r>(
        &self,
        request: &MaintenanceRequest<'r>,
        run: &mut RetentionRun<'_>,
    ) -> Result<Observation<'r>, Error> {
        if request.now < 0 || !(1..=256).contains(&request.max_purges) {
            return Err(navo_core::retention::RetentionError::Invalid(
                "invalid maintenance bounds",
            )
            .into());
        }
        run.visit(request.at)?;
        Ok(Observation {
            at: request.at,
            identity: proof::identity(self.ops, request.at)?,
            mount: proof::scope(self.ops, request.at)?.mount,
        })
    }

    fn expire(
        &mut self,
        id: RetentionId,
        resume: bool,
        run: &mut RetentionRun<'_>,
    ) -> Result<Attempt, Error> {
        let completed = if resume {
            self.reconcile(id, run)
        } else {
            self.purge(id, run)
        };
        match completed {
            Ok(entry) if entry.state == RetainedState::Purged => Ok(Attempt::Purged),
            Ok(entry) if resume && entry.state != RetainedState::Purging => Ok(Attempt::Settled),
            Ok(_) => skip(id, Error::State(id), run).map(|()| Attempt::Skipped),
            Err(error) => skip(id, error, run).map(|()| Attempt::Skipped),
        }
    }

    fn refresh(
        &self,
        inventory: &mut [RetentionSummary],
        settled: RetentionSummary,
    ) -> Result<(), Error> {
        let entry = self.catalog.get(settled.id())?;
        let mut fresh =
            RetentionSummary::try_from(&entry)?.with_attempts(settled.attempts().saturating_add(1));
        fresh.protect();
        for slot in inventory
            .iter_mut()
            .filter(|slot| slot.id() == settled.id())
        {
            *slot = fresh;
        }
        Ok(())
    }

    fn cleanup_terminal(
        &mut self,
        observed: &Observation<'_>,
        (limit, run): (u16, &mut RetentionRun<'_>),
    ) -> Result<(Vec<RetentionId>, u16), Error> {
        let ids = self
            .catalog
            .pending_cleanup(observed.identity.volume, u32::from(limit))?;
        let mut skipped = Vec::new();
        let mut attempts = 0;
        for id in ids {
            let entry = self.catalog.get(id)?;
            if entry.record.parent_identity.volume != observed.identity.volume {
                continue;
            }
            run.visit(&entry.record.container)?;
            observed.measure(self.ops)?;
            attempts += 1;
            self.catalog.begin_cleanup_attempt(id)?;
            if let Err(error) = self.check_home(&entry).and_then(|()| self.cleanup(&entry)) {
                skip(id, error, run)?;
                skipped.push(id);
            }
        }
        Ok((skipped, attempts))
    }

    fn inventory(
        &self,
        (at, run): (&Location, &mut RetentionRun<'_>),
        protected: &HashSet<RetentionId>,
    ) -> Result<Vec<RetentionSummary>, Error> {
        let mut entries = Vec::new();
        let mut after = 0;
        loop {
            run.visit(at)?;
            let mut page = self.catalog.summaries_after(after, 1000)?;
            for entry in page
                .iter_mut()
                .filter(|entry| protected.contains(&entry.id()))
            {
                entry.protect();
            }
            if page.is_empty() {
                return Ok(entries);
            }
            if entries.len() + page.len() > MAX_NODES {
                return Err(navo_core::retention::RetentionError::Invalid(
                    "maintenance inventory is too large",
                )
                .into());
            }
            after = page.last().map_or(after, |entry| entry.id().0);
            entries.extend(page);
        }
    }
}

impl Observation<'_> {
    fn measure(&self, ops: &dyn navo_fs::ProviderOps) -> Result<VolumeSpace, Error> {
        self.verify(ops)?;
        let free_bytes = ops
            .available_space(self.at)?
            .ok_or_else(|| Error::Unsupported(self.at.clone()))?;
        self.verify(ops)?;
        Ok(VolumeSpace {
            volume: self.identity.volume,
            free_bytes,
        })
    }

    fn verify(&self, ops: &dyn navo_fs::ProviderOps) -> Result<(), Error> {
        proof::folder(ops, self.at, self.identity)?;
        if proof::scope(ops, self.at)?.mount != self.mount {
            return Err(Error::Changed(self.at.clone()));
        }
        Ok(())
    }
}

fn cancelled(error: &Error, run: &RetentionRun<'_>) -> bool {
    run.cancel.is_cancelled()
        || matches!(error, Error::Fs(fs) if fs.kind == navo_core::FsErrorKind::Cancelled)
}

pub(super) fn skip(id: RetentionId, error: Error, run: &RetentionRun<'_>) -> Result<(), Error> {
    if cancelled(&error, run) || matches!(error, Error::Catalog(_) | Error::Lock(_)) {
        return Err(error);
    }
    tracing::warn!(
        id = id.0,
        error_kind = error.kind(),
        "retained entry skipped by maintenance"
    );
    Ok(())
}

enum Attempt {
    Purged,
    Settled,
    Skipped,
}

/// Interrupted work resumes first unless a fresh purge choice has fewer failed attempts.
fn next_attempt(
    inventory: &[RetentionSummary],
    plan: &PurgePlan,
    volume: u64,
) -> Option<RetentionSummary> {
    let resume = inventory
        .iter()
        .filter(|entry| entry.resumes(volume))
        .min_by_key(|entry| entry.attempts());
    let choice = plan
        .choices
        .first()
        .and_then(|choice| inventory.iter().find(|entry| entry.id() == choice.id));
    match (resume, choice) {
        (Some(resume), Some(choice)) if choice.attempts() < resume.attempts() => Some(*choice),
        (Some(resume), _) => Some(*resume),
        (None, choice) => choice.copied(),
    }
}

fn exclude_from_selection(inventory: &mut [RetentionSummary], id: RetentionId) {
    for entry in inventory.iter_mut().filter(|entry| entry.id() == id) {
        entry.protect();
    }
}

fn report(
    purged: Vec<RetentionId>,
    inventory: &[RetentionSummary],
    (request, space, budget_exhausted): (&MaintenanceRequest<'_>, VolumeSpace, bool),
) -> MaintenanceReport {
    let remaining_bytes = inventory
        .iter()
        .map(|entry| u128::from(entry.bytes_on(space.volume)))
        .sum();
    MaintenanceReport {
        purged,
        skipped: Vec::new(),
        remaining_bytes,
        quota_unmet: remaining_bytes > u128::from(request.policy.max_volume_bytes()),
        low_space: space.free_bytes < request.policy.min_free_bytes(),
        budget_exhausted,
        cleanup_pending: false,
    }
}
