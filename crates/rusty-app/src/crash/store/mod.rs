use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusty_fs::{LocalFs, ProviderOps};
use rusty_store::{DbError, JournalError, RecoveryError};

pub(super) mod access;
mod build;
mod details;
mod files;
mod legacy;
mod locks;
mod notice;
mod promote;
mod refresh;
mod state;
mod workflow;

pub(crate) use notice::RecoveryNotice;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Db(#[from] DbError),
    #[error("{0}")]
    Journal(#[from] JournalError),
    #[error("{0}")]
    Recovery(#[from] RecoveryError),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Default)]
pub(crate) struct StoreStart {
    path: Option<PathBuf>,
    locks: Option<Arc<locks::Locks>>,
    _legacy: Option<std::fs::File>,
    pub notice: Option<RecoveryNotice>,
}

impl StoreStart {
    pub fn share(&mut self) {
        if let Some(locks) = &self.locks
            && let Err(error) = locks.share()
        {
            if let Some(path) = &self.path {
                access::block(path, &error.to_string());
                super::record_failed_check(path);
                self.notice = Some(RecoveryNotice::failed(
                    path.parent().unwrap_or(Path::new(".")),
                    &error.to_string(),
                ));
            }
            tracing::error!(%error, "cannot share the database session");
        }
    }
}

pub(crate) fn check_store(data: Option<&Path>, unclean: bool, experimental: bool) -> StoreStart {
    check_with(data, unclean, &LocalFs::new(), experimental)
}

pub(crate) fn check_with(
    data: Option<&Path>,
    unclean: bool,
    ops: &dyn ProviderOps,
    experimental: bool,
) -> StoreStart {
    if !experimental {
        return legacy::start(data, unclean, ops);
    }
    let Some(data) = data else {
        return StoreStart::default();
    };
    let path = rusty_store::StoreDirs::journal_in(data);
    let acquired = std::fs::create_dir_all(data).and_then(|()| locks::Locks::acquire(data));
    let locks = match acquired {
        Ok(locks) => locks,
        Err(error) => {
            access::block(&path, &error.to_string());
            return StoreStart {
                path: Some(path),
                locks: None,
                _legacy: None,
                notice: Some(RecoveryNotice::failed(data, &error.to_string())),
            };
        }
    };
    access::register(&path, &locks);
    let outcome = workflow::run(
        data,
        workflow::Mode {
            unclean,
            experimental,
            sole: locks.sole,
        },
        ops,
    );
    let notice = match outcome {
        Ok(notice) => {
            access::allow(&path);
            notice
        }
        Err(error) => {
            access::block(&path, &error.to_string());
            super::record_failed_check(&path);
            tracing::error!(%error, database = %path.display(), "database startup recovery stopped");
            Some(RecoveryNotice::failed(data, &error.to_string()))
        }
    };
    StoreStart {
        path: Some(path),
        locks: Some(locks),
        _legacy: None,
        notice,
    }
}

#[cfg(test)]
mod tests;
