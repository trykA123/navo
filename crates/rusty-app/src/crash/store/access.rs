use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use rusty_store::DbError;

use super::locks::Locks;

struct Access {
    lease: Weak<Locks>,
    blocked: Option<String>,
}

static ACCESS: OnceLock<Mutex<HashMap<PathBuf, Access>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<PathBuf, Access>> {
    ACCESS.get_or_init(Mutex::default)
}

fn key(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

pub(super) fn register(path: &Path, lease: &Arc<Locks>) {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            key(path),
            Access {
                lease: Arc::downgrade(lease),
                blocked: Some("database startup has not completed".into()),
            },
        );
}

pub(super) fn allow(path: &Path) {
    if let Some(access) = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_mut(&key(path))
    {
        access.blocked = None;
    }
}

pub(super) fn block(path: &Path, message: &str) {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let access = registry.entry(key(path)).or_insert_with(|| Access {
        lease: Weak::new(),
        blocked: None,
    });
    access.blocked = Some(message.into());
}

pub(crate) fn lease(path: &Path) -> Result<Option<Arc<dyn Send + Sync>>, DbError> {
    let registry = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(access) = registry.get(&key(path)) else {
        return Ok(None);
    };
    if let Some(message) = &access.blocked {
        return Err(DbError::Corrupt(message.clone()));
    }
    let lease = access
        .lease
        .upgrade()
        .ok_or_else(|| DbError::Corrupt("the database session has closed".into()))?;
    Ok(Some(lease))
}
