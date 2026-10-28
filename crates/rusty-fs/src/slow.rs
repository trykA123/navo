use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use rusty_core::{FsError, Location, ProviderId};

use crate::{
    CancelToken, Capabilities, CopyControl, FileDetails, FileIdentity, FsEntry, ListSink,
    OwnedCopy, Provider, ProviderOps, SizeEntry, Watch, WatchSink,
};

pub struct SlowFs {
    inner: Arc<dyn ProviderOps>,
    delay: Duration,
}

impl SlowFs {
    #[must_use]
    pub fn new(inner: Arc<dyn ProviderOps>, delay: Duration) -> Self {
        Self { inner, delay }
    }

    #[must_use]
    pub fn parse_delay_ms(value: Option<&str>) -> Option<Duration> {
        value?
            .parse::<u64>()
            .ok()
            .filter(|&millis| millis > 0)
            .map(Duration::from_millis)
    }

    fn sleep(&self) {
        std::thread::sleep(self.delay);
    }
}

impl Provider for SlowFs {
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
        self.sleep();
        self.inner.list(folder, sink, cancel)
    }

    fn list_sizes(
        &self,
        folder: &Location,
        sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.sleep();
        self.inner.list_sizes(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.sleep();
        self.inner.metadata(location)
    }

    fn details(&self, location: &Location) -> Result<FileDetails, FsError> {
        self.sleep();
        self.inner.details(location)
    }

    fn watch(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.sleep();
        self.inner.watch(folder, sink)
    }

    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.sleep();
        self.inner.watch_tree(folder, sink)
    }
}

impl ProviderOps for SlowFs {
    fn hide_retained(&self, location: &Location, identity: FileIdentity) -> Result<(), FsError> {
        self.sleep();
        self.inner.hide_retained(location, identity)
    }

    fn copy_file_owned(
        &self,
        request: OwnedCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        self.sleep();
        self.inner.copy_file_owned(request, control)
    }

    fn remove_owned_file(
        &self,
        location: &Location,
        identity: FileIdentity,
    ) -> Result<(), FsError> {
        self.sleep();
        self.inner.remove_owned_file(location, identity)
    }

    fn available_space(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.sleep();
        self.inner.available_space(location)
    }

    fn supports_atomic_rename(&self) -> bool {
        self.inner.supports_atomic_rename()
    }

    fn rename_atomic(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.rename_atomic(source, target)
    }

    fn mount_identity(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.sleep();
        self.inner.mount_identity(location)
    }

    fn file_identity(&self, location: &Location) -> Result<Option<FileIdentity>, FsError> {
        self.sleep();
        self.inner.file_identity(location)
    }

    fn matches_identity(
        &self,
        location: &Location,
        expected: Option<FileIdentity>,
    ) -> Result<bool, FsError> {
        self.sleep();
        self.inner.matches_identity(location, expected)
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.sleep();
        self.inner.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.recycle(location)
    }

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        self.sleep();
        self.inner.make_link(target_of, at)
    }

    fn link_target(&self, at: &Location) -> Result<Option<Location>, FsError> {
        self.sleep();
        self.inner.link_target(at)
    }

    fn link_points_at(&self, at: &Location, target_of: &Location) -> Result<bool, FsError> {
        self.sleep();
        self.inner.link_points_at(at, target_of)
    }

    fn volume_serial(&self, location: &Location) -> Result<Option<u64>, FsError> {
        self.sleep();
        self.inner.volume_serial(location)
    }
}

#[cfg(test)]
mod tests;
