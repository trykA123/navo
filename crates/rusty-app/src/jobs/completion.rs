use rusty_fs::ProviderOps;
use rusty_store::JobState;

use super::{JournalLog, held, lock, overwrite, session};

impl JournalLog {
    pub(super) fn complete(&self, state: JobState, ops: &dyn ProviderOps) -> Result<(), String> {
        if self.overwrites.is_some() {
            session::keep(self, session::Kept::Retained);
            return overwrite::finish(self, state, ops);
        }
        if self.session.is_some() {
            return session::finish(self, state, ops);
        }
        if !self.experimental_retention {
            self.complete_like_main(state, ops);
            return Ok(());
        }
        lock(&self.journal)
            .finish(self.job, state)
            .map_err(|error| error.to_string())?;
        if !self.stranded {
            held::release(&self.journal, self.job, &self.held, ops)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

impl JournalLog {
    fn complete_like_main(&self, state: JobState, ops: &dyn ProviderOps) {
        if let Err(error) = lock(&self.journal).finish(self.job, state) {
            tracing::error!(job = self.job.0, %error, "recording the finished job failed");
            return;
        }
        if !self.stranded
            && let Err(error) = held::release(&self.journal, self.job, &self.held, ops)
        {
            tracing::warn!(job = self.job.0, %error, "releasing held files failed");
        }
    }
}
