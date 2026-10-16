use std::sync::Arc;

use gpui_kit::ScrollHandle;
use rusty_core::Location;
use rusty_fs::{
    FolderProfile, FsEntry, JobSlot, OwnerGuard, Priority, ProfileHandle, ProfileState, Provider,
    load_folder, spawn_profile, volume_of,
};
use rusty_structured::NodeId;

use super::contents::Contents;
use super::follow::Target;
use super::inbox::Arrival;
use super::scan::{Scan, Snapshots};
use super::{Preview, Shown};
use crate::browser::Measured;
use crate::prelude::*;
use crate::primitives::ListScroll;
use crate::window_view::FerriteWindow;

struct Read {
    node: NodeId,
    _owner: OwnerGuard,
}

pub(in crate::window_view) struct FolderPreview {
    pub target: Target,
    pub contents: Contents,
    pub scan: Scan,
    pub tree_scroll: ListScroll,
    pub summary_scroll: ScrollHandle,
    load: u64,
    reads: Vec<Read>,
}

impl FolderPreview {
    pub(super) fn stop(&mut self) -> bool {
        let had_reads = !self.reads.is_empty();
        self.reads.clear();
        self.scan.stop() || had_reads
    }
}

impl Preview {
    pub(super) fn folder_mut(&mut self) -> Option<&mut FolderPreview> {
        match &mut self.shown {
            Some(Shown::Folder(folder)) => Some(folder),
            _ => None,
        }
    }

    pub(super) fn folder(&self) -> Option<&FolderPreview> {
        match &self.shown {
            Some(Shown::Folder(folder)) => Some(folder),
            _ => None,
        }
    }

    fn next_id(&mut self) -> u64 {
        self.ids += 1;
        self.ids
    }

    fn scan(
        &self,
        provider: Arc<dyn Provider>,
        folder: &Location,
        ids: (u64, u64),
    ) -> ProfileHandle {
        let (load, run) = ids;
        let sink = Snapshots {
            send: self.inbox.send.clone(),
            wake: Arc::clone(&self.inbox.wake),
            load,
            run,
            every: self.every,
        };
        spawn_profile(&self.scans, provider, folder.clone(), sink)
    }

    fn read(&self, provider: Arc<dyn Provider>, request: ReadRequest) -> Read {
        let ReadRequest {
            load,
            node,
            location,
            show_hidden,
        } = request;
        let owner = self.reads.owner();
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Visible,
            owner: owner.id(),
        };
        let (send, wake) = (self.inbox.send.clone(), Arc::clone(&self.inbox.wake));
        self.reads.submit(
            slot,
            Box::new(move |cancel| {
                let listed = load_folder(provider.as_ref(), &location, cancel)
                    .map(|snapshot| {
                        snapshot
                            .details
                            .into_iter()
                            .filter(|entry| show_hidden || !entry.attributes.hidden)
                            .collect()
                    })
                    .map_err(|error| error.message());
                if send.send(Arrival::Read { load, node, listed }).is_ok() {
                    wake();
                }
            }),
        );
        Read {
            node,
            _owner: owner,
        }
    }
}

struct ReadRequest {
    load: u64,
    node: NodeId,
    location: Location,
    show_hidden: bool,
}

impl FerriteWindow {
    fn provider_for_preview(&self) -> Arc<dyn Provider> {
        self.provider.clone()
    }

    pub(super) fn start_folder(&mut self, target: Target) -> FolderPreview {
        let load = self.preview.next_id();
        let contents = Contents::new(target.location.clone());
        let cached = self
            .browser
            .profiles()
            .get(&target.location, target.changed_ms());
        let scan = match cached {
            Some(profile) => {
                self.show_size(&target, profile.totals.bytes);
                Scan::cached(FolderProfile::clone(&profile))
            }
            None => {
                let run = self.preview.next_id();
                let provider = self.provider_for_preview();
                let handle = self.preview.scan(provider, &target.location, (load, run));
                Scan::counting(handle, run)
            }
        };
        let reads = contents
            .root()
            .map(|(node, location)| self.read_folder(load, node, location))
            .into_iter()
            .collect();
        FolderPreview {
            target,
            contents,
            scan,
            tree_scroll: ListScroll::new(),
            summary_scroll: ScrollHandle::new(),
            load,
            reads,
        }
    }

    fn read_folder(&self, load: u64, node: NodeId, location: Location) -> Read {
        let request = ReadRequest {
            load,
            node,
            location,
            show_hidden: self.quick.show_hidden,
        };
        self.preview.read(self.provider_for_preview(), request)
    }

    pub(in crate::window_view) fn poll_preview(&mut self) -> bool {
        let mut changed = false;
        for arrival in self.preview.inbox.take() {
            changed |= match arrival {
                Arrival::Read { load, node, listed } => self.folder_read(load, node, listed),
                Arrival::Snapshot { load, run, profile } => {
                    self.profile_arrived(load, run, profile)
                }
            };
        }
        changed
    }

    fn folder_read(
        &mut self,
        load: u64,
        node: NodeId,
        listed: Result<Vec<FsEntry>, String>,
    ) -> bool {
        let Some(folder) = self
            .preview
            .folder_mut()
            .filter(|folder| folder.load == load)
        else {
            return false;
        };
        if !folder.reads.iter().any(|read| read.node == node) {
            return false;
        }
        folder.contents.attach(node, listed);
        folder.reads.retain(|read| read.node != node);
        true
    }

    fn profile_arrived(&mut self, load: u64, run: u64, profile: FolderProfile) -> bool {
        let Some(folder) = self
            .preview
            .folder_mut()
            .filter(|folder| folder.load == load)
        else {
            return false;
        };
        let is_done = profile.state == ProfileState::Done;
        if !folder.scan.accept(run, profile) {
            return false;
        }
        if is_done && let Some(profile) = folder.scan.profile.clone() {
            let target = folder.target.clone();
            self.browser.profiles().put(
                target.location.clone(),
                target.changed_ms(),
                profile.clone(),
            );
            self.show_size(&target, profile.totals.bytes);
        }
        true
    }

    fn show_size(&mut self, target: &Target, bytes: u64) {
        let _ = self.browser.show_folder_size(&Measured {
            tab: target.tab,
            folder: target.folder.clone(),
            name: target.name(),
            bytes,
        });
    }

    pub(in crate::window_view) fn stop_scan(&mut self, cx: &mut Context<Self>) -> bool {
        let stopped = self.preview.is_open
            && self
                .preview
                .folder_mut()
                .is_some_and(|folder| folder.scan.stop());
        if stopped {
            cx.notify();
        }
        stopped
    }

    pub(super) fn resume_scan(&mut self, cx: &mut Context<Self>) {
        let Some((load, location)) = self
            .preview
            .folder()
            .filter(|folder| !folder.scan.is_counting())
            .map(|folder| (folder.load, folder.target.location.clone()))
        else {
            return;
        };
        let run = self.preview.next_id();
        let handle = self
            .preview
            .scan(self.provider_for_preview(), &location, (load, run));
        if let Some(folder) = self.preview.folder_mut() {
            folder.scan.resume(handle, run);
        }
        cx.notify();
    }

    pub(super) fn toggle_contents(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(folder) = self.preview.folder_mut() else {
            return;
        };
        let load = folder.load;
        if let Some((node, location)) = folder.contents.toggle(index) {
            let read = self.read_folder(load, node, location);
            if let Some(folder) = self.preview.folder_mut() {
                folder.reads.push(read);
            }
        }
        cx.notify();
    }
}
