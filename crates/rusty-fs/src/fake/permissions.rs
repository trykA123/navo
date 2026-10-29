use std::collections::HashMap;

use rusty_core::{FsError, Location};

use super::ops::fail_now;
use super::{FakeFs, FakeOp, lock, normalize};
use crate::UnixAccess;

#[derive(Debug, Default)]
pub(super) struct Permissions {
    access: HashMap<String, UnixAccess>,
    owned: HashMap<String, bool>,
    fixed: bool,
}

impl FakeFs {
    pub fn set_access(&self, path: &str, access: UnixAccess) {
        lock(&self.permissions)
            .access
            .insert(normalize(path).to_owned(), access);
    }

    pub fn fix_permissions(&self) {
        lock(&self.permissions).fixed = true;
    }

    pub fn set_owned_by_current_user(&self, path: &str, owned: bool) {
        lock(&self.permissions)
            .owned
            .insert(normalize(path).to_owned(), owned);
    }

    pub(super) fn access_of(&self, location: &Location) -> Result<Option<UnixAccess>, FsError> {
        let path = self.own_path(location)?;
        Ok(lock(&self.permissions).access.get(&path).copied())
    }

    pub(super) fn owner_of(&self, location: &Location) -> Result<Option<bool>, FsError> {
        let path = self.own_path(location)?;
        Ok(lock(&self.permissions).owned.get(&path).copied())
    }

    pub(super) fn restrict(&self, location: &Location) -> Result<(), FsError> {
        let path = self.own_path(location)?;
        fail_now(self.record_call(&path, FakeOp::Secure), location)?;
        let mut permissions = lock(&self.permissions);
        if permissions.fixed {
            return Ok(());
        }
        if let Some(access) = permissions.access.get_mut(&path) {
            access.mode &= !0o777;
            access.mode |= 0o700;
        }
        Ok(())
    }

    pub(super) fn record_secure(&self, location: &Location) -> Result<(), FsError> {
        let path = self.own_path(location)?;
        fail_now(self.record_call(&path, FakeOp::Secure), location)
    }
}
