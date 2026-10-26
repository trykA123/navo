use std::collections::HashSet;

use rusty_core::retention::{
    MAX_NODES, PurgeRequest, RetentionId, RetentionPolicy, RetentionSummary, VolumeSpace,
    select_purge_inventory,
};
use rusty_core::{FileIdentity, Location};

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
        request: MaintenanceRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<MaintenanceReport, Error> {
        self.maintain_inner(request, run, &HashSet::new())
    }

    pub fn maintain_job_volume(
        &mut self,
        request: MaintenanceRequest<'_>,
        job: rusty_store::JobId,
        run: &mut RetentionRun<'_>,
    ) -> Result<MaintenanceReport, Error> {
        let protected = self.catalog.ids_for_job(job)?.into_iter().collect();
        self.maintain_inner(request, run, &protected)
    }

    fn maintain_inner(
        &mut self,
        request: MaintenanceRequest<'_>,
        run: &mut RetentionRun<'_>,
        protected: &HashSet<RetentionId>,
    ) -> Result<MaintenanceReport, Error> {
        if request.now < 0 || !(1..=256).contains(&request.max_purges) {
            return Err(rusty_core::retention::RetentionError::Invalid(
                "invalid maintenance bounds",
            )
            .into());
        }
        run.visit(request.at)?;
        let observed = Observation {
            at: request.at,
            identity: proof::identity(self.ops, request.at)?,
            mount: proof::scope(self.ops, request.at)?.mount,
        };
        let mut inventory = self.inventory((request.at, run), protected)?;
        self.cleanup_terminal(&observed, (u32::from(request.max_purges), run))?;
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
            let resume = inventory
                .iter()
                .find(|entry| entry.resumes_purge(space.volume))
                .map(|entry| entry.id());
            let next = resume.or_else(|| plan.choices.first().map(|choice| choice.id));
            if next.is_none() || purged.len() == usize::from(request.max_purges) {
                let mut result = report(purged, &inventory, (&request, space, next.is_some()));
                result.cleanup_pending = !self.catalog.pending_cleanup(space.volume, 1)?.is_empty();
                return Ok(result);
            }
            let id = next.ok_or(rusty_core::retention::RetentionError::Invalid(
                "missing maintenance candidate",
            ))?;
            let completed = if resume.is_some() {
                self.reconcile(id, run)?
            } else {
                self.purge(id, run)?
            };
            if completed.state != rusty_core::retention::RetainedState::Purged {
                return Err(Error::State(id));
            }
            inventory.retain(|entry| entry.id() != id);
            purged.push(id);
        }
    }

    fn cleanup_terminal(
        &mut self,
        observed: &Observation<'_>,
        (limit, run): (u32, &mut RetentionRun<'_>),
    ) -> Result<(), Error> {
        let ids = self
            .catalog
            .pending_cleanup(observed.identity.volume, limit)?;
        for id in ids {
            let entry = self.catalog.get(id)?;
            if entry.record.parent_identity.volume != observed.identity.volume {
                continue;
            }
            run.visit(&entry.record.container)?;
            observed.measure(self.ops)?;
            self.catalog.begin_cleanup_attempt(id)?;
            self.cleanup(&entry)?;
        }
        Ok(())
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
                return Err(rusty_core::retention::RetentionError::Invalid(
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
    fn measure(&self, ops: &dyn rusty_fs::ProviderOps) -> Result<VolumeSpace, Error> {
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

    fn verify(&self, ops: &dyn rusty_fs::ProviderOps) -> Result<(), Error> {
        proof::folder(ops, self.at, self.identity)?;
        if proof::scope(ops, self.at)?.mount != self.mount {
            return Err(Error::Changed(self.at.clone()));
        }
        Ok(())
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
        remaining_bytes,
        quota_unmet: remaining_bytes > u128::from(request.policy.max_volume_bytes()),
        low_space: space.free_bytes < request.policy.min_free_bytes(),
        budget_exhausted,
        cleanup_pending: false,
    }
}
