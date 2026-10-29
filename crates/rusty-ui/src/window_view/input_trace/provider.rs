use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rusty_core::{FsError, Location, ProviderId};

use rusty_fs::{
    CancelToken, Capabilities, CopyControl, FileDetails, FileIdentity, FsEntry, ListSink,
    OwnedCopy, Provider, ProviderOps, SizeEntry, UnixAccess, Watch, WatchSink,
};

pub(super) struct CountingFs {
    inner: Arc<dyn ProviderOps>,
    calls: Arc<AtomicU64>,
}

impl CountingFs {
    pub(super) fn new(inner: Arc<dyn ProviderOps>, calls: Arc<AtomicU64>) -> Self {
        Self { inner, calls }
    }

    fn count(&self) {
        self.calls.fetch_add(1, Ordering::Relaxed);
    }
}

impl Provider for CountingFs {
    fn id(&self) -> ProviderId {
        self.inner.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.count();
        self.inner.list(folder, sink, cancel)
    }

    fn list_sizes(
        &self,
        folder: &Location,
        sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.count();
        self.inner.list_sizes(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.count();
        self.inner.metadata(location)
    }

    fn details(&self, location: &Location) -> Result<FileDetails, FsError> {
        self.count();
        self.inner.details(location)
    }

    fn watch(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.count();
        self.inner.watch(folder, sink)
    }

    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.count();
        self.inner.watch_tree(folder, sink)
    }
}

impl ProviderOps for CountingFs {
    fn hide_retained(&self, location: &Location, identity: FileIdentity) -> Result<(), FsError> {
        self.count();
        self.inner.hide_retained(location, identity)
    }

    fn copy_file_owned(
        &self,
        request: OwnedCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        self.count();
        self.inner.copy_file_owned(request, control)
    }

    fn remove_owned_file(
        &self,
        location: &Location,
        identity: FileIdentity,
    ) -> Result<(), FsError> {
        self.count();
        self.inner.remove_owned_file(location, identity)
    }

    fn available_space(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.count();
        self.inner.available_space(location)
    }

    fn supports_atomic_rename(&self) -> bool {
        self.inner.supports_atomic_rename()
    }

    fn rename_atomic(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.rename_atomic(source, target)
    }

    fn mount_identity(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.count();
        self.inner.mount_identity(location)
    }

    fn file_identity(&self, location: &Location) -> Result<Option<FileIdentity>, FsError> {
        self.count();
        self.inner.file_identity(location)
    }

    fn matches_identity(
        &self,
        location: &Location,
        expected: Option<FileIdentity>,
    ) -> Result<bool, FsError> {
        self.count();
        self.inner.matches_identity(location, expected)
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.create_folder(location)
    }

    fn create_private_folder(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.create_private_folder(location)
    }

    fn unix_access(&self, location: &Location) -> Result<Option<UnixAccess>, FsError> {
        self.count();
        self.inner.unix_access(location)
    }

    fn restrict_to_owner(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.restrict_to_owner(location)
    }

    fn owned_by_current_user(&self, location: &Location) -> Result<Option<bool>, FsError> {
        self.count();
        self.inner.owned_by_current_user(location)
    }

    fn volume_root(&self, location: &Location) -> Result<Option<Location>, FsError> {
        self.count();
        self.inner.volume_root(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.count();
        self.inner.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.recycle(location)
    }

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        self.count();
        self.inner.make_link(target_of, at)
    }

    fn link_target(&self, at: &Location) -> Result<Option<Location>, FsError> {
        self.count();
        self.inner.link_target(at)
    }

    fn link_points_at(&self, at: &Location, target_of: &Location) -> Result<bool, FsError> {
        self.count();
        self.inner.link_points_at(at, target_of)
    }

    fn volume_serial(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.count();
        self.inner.volume_serial(location)
    }
}
