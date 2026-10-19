use std::sync::Arc;

use gpui_kit::ScrollHandle;
use rusty_core::Location;
use rusty_fs::{
    FolderProfile, FsEntry, JobSlot, LiveHandle, LiveRequest, LiveSnapshot, OwnerGuard, Priority,
    ProfileState, Provider, load_folder, spawn_live, volume_of,
};
use rusty_structured::NodeId;

use super::contents::{CHILD_LIMIT, Contents};
use super::follow::Target;
use super::inbox::Arrival;
use super::marks::Marks;
use super::scan::{Scan, Snapshots};
use super::spot::{Restore, Spot};
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
    pub spot: Option<Spot>,
    pub restore: Option<Restore>,
    pub marks: Marks,
    load: u64,
    reads: Vec<Read>,
}

impl FolderPreview {
    pub(super) fn stop(&mut self) -> bool {
        let had_reads = !self.reads.is_empty();
        self.reads.clear();
        self.scan.end() || had_reads
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

    fn scan(&self, request: LiveRequest, run: u64) -> LiveHandle {
        let sink = Snapshots {
            send: self.inbox.send.clone(),
            wake: Arc::clone(&self.inbox.wake),
            run,
            every: self.every,
        };
        spawn_live(&self.scans, request, sink)
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
                        let mut entries: Vec<FsEntry> = snapshot
                            .details
                            .into_iter()
                            .filter(|entry| show_hidden || !entry.attributes.hidden)
                            .collect();
                        let hidden = entries.len().saturating_sub(CHILD_LIMIT);
                        entries.truncate(CHILD_LIMIT);
                        (entries, hidden)
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
    pub(super) fn provider_for_preview(&self) -> Arc<dyn Provider> {
        self.provider.clone()
    }

    pub(super) fn start_folder(&mut self, target: Target, carried: Option<Scan>) -> FolderPreview {
        let load = self.preview.next_id();
        let contents = Contents::new(target.location.clone());
        let scan = carried.unwrap_or_else(|| self.start_scan(&target));
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
            spot: None,
            restore: None,
            marks: Marks::default(),
            load,
            reads,
        }
    }

    fn start_scan(&mut self, target: &Target) -> Scan {
        let cached = self
            .browser
            .profiles()
            .get(&target.location, target.changed_ms());
        if let Some(profile) = &cached {
            self.show_size(target, profile.totals.bytes);
        }
        let base = cached.map(|profile| FolderProfile::clone(&profile));
        let run = self.preview.next_id();
        let request = LiveRequest {
            provider: self.provider_for_preview(),
            folder: target.location.clone(),
            base: base.clone(),
        };
        let handle = self.preview.scan(request, run);
        match base {
            Some(profile) => Scan::cached(profile, handle, run),
            None => Scan::counting(handle, run),
        }
    }

    fn read_folder(&self, load: u64, node: NodeId, location: Location) -> Read {
        let request = ReadRequest {
            load,
            node,
            location,
            show_hidden: self.values.show_hidden(),
        };
        self.preview.read(self.provider_for_preview(), request)
    }

    pub(in crate::window_view) fn poll_preview(&mut self) -> bool {
        let mut changed = false;
        for arrival in self.preview.inbox.take() {
            changed |= match arrival {
                Arrival::Read { load, node, listed } => self.folder_read(load, node, listed),
                Arrival::Snapshot { run, snapshot } => self.profile_arrived(run, snapshot),
                Arrival::Counted { location, bytes } => self.folder_counted(&location, bytes),
            };
        }
        changed
    }

    fn folder_read(
        &mut self,
        load: u64,
        node: NodeId,
        listed: Result<(Vec<FsEntry>, usize), String>,
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
        let deeper = folder.contents.attach(node, listed);
        folder.settle_spot();
        folder.reads.retain(|read| read.node != node);
        self.read_more(deeper);
        true
    }

    fn read_more(&mut self, wanted: Vec<(NodeId, Location)>) {
        let Some(load) = self.preview.folder().map(|folder| folder.load) else {
            return;
        };
        let reads: Vec<Read> = wanted
            .into_iter()
            .map(|(node, location)| self.read_folder(load, node, location))
            .collect();
        if let Some(folder) = self.preview.folder_mut() {
            folder.reads.extend(reads);
        }
    }

    pub(super) fn fold_below(&mut self, is_opening: bool, cx: &mut Context<Self>) {
        let Some(folder) = self.preview.folder_mut() else {
            return;
        };
        let Some(index) = folder.spot_row() else {
            return;
        };
        if is_opening {
            let wanted = folder.contents.open_below(index);
            self.read_more(wanted);
        } else {
            folder.contents.close_below(index);
        }
        cx.notify();
    }

    fn profile_arrived(&mut self, run: u64, snapshot: LiveSnapshot) -> bool {
        let Some(folder) = self.preview.folder_mut() else {
            return false;
        };
        let is_done = snapshot.profile.state == ProfileState::Done;
        let is_recounting = snapshot.is_recounting;
        if !folder.scan.accept(run, snapshot) {
            return false;
        }
        if is_done && let Some(profile) = folder.scan.profile.clone() {
            let target = folder.target.clone();
            let profiles = self.browser.profiles();
            if is_recounting {
                profiles.invalidate(&target.location);
            } else {
                profiles.put(
                    target.location.clone(),
                    target.changed_ms(),
                    profile.clone(),
                );
            }
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
        let Some(location) = self
            .preview
            .folder()
            .filter(|folder| !folder.scan.is_counting())
            .map(|folder| folder.target.location.clone())
        else {
            return;
        };
        let run = self.preview.next_id();
        let request = LiveRequest {
            provider: self.provider_for_preview(),
            folder: location,
            base: None,
        };
        let handle = self.preview.scan(request, run);
        if let Some(folder) = self.preview.folder_mut() {
            folder.scan.resume(handle, run);
        }
        cx.notify();
    }

    pub(super) fn recount_folder(&mut self, cx: &mut Context<Self>) {
        let is_asked = self
            .preview
            .folder()
            .is_some_and(|folder| folder.scan.recount());
        if !is_asked {
            self.resume_scan(cx);
        }
    }

    pub(super) fn toggle_contents(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(folder) = self.preview.folder_mut() else {
            return;
        };
        let wanted = folder.contents.toggle(index).into_iter().collect();
        self.read_more(wanted);
        cx.notify();
    }
}
