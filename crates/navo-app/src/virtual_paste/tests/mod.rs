use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navo_core::{ConflictPolicy, ItemKind, Location, OpKind, OperationPlan, PlanRequest, plan};
use navo_fs::{
    CancelToken, JobContext, JobOutcome, LocalFs, Progress, existing_at,
    run_job_parallel_with_sources,
};
use navo_shell::{
    Backend, BadName, ItemSource, Opened, SourceKind, VirtualClipboard, VirtualEntry,
};
use navo_ui::browser::{Operations, StreamPaste};

use super::stream_paste;

mod browser;
mod jobs;
mod recovery;

pub(super) const TIME: u64 = 1_600_000_000;

pub(super) fn time(offset: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(TIME + offset)
}

#[derive(Clone)]
pub(super) enum Then {
    End,
    Fail(&'static str),
    Hang,
}

#[derive(Clone)]
pub(super) enum Feed {
    Bytes(Vec<u8>, Then),
    Storage(Vec<u8>),
}

/// A source that serves fixed items and records which ones were opened.
pub(super) struct Fake {
    pub feeds: Vec<Feed>,
    pub opened: Arc<Mutex<Vec<u32>>>,
}

struct Steps {
    data: Vec<u8>,
    at: usize,
    then: Then,
}

impl ItemSource for Steps {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        if self.at < self.data.len() {
            let count = (self.data.len() - self.at).min(buf.len()).min(4096);
            buf[..count].copy_from_slice(&self.data[self.at..self.at + count]);
            self.at += count;
            return Ok(count);
        }
        match self.then {
            Then::End => Ok(0),
            Then::Fail(message) => Err(message.to_owned()),
            Then::Hang => {
                std::thread::sleep(Duration::from_secs(3));
                Ok(0)
            }
        }
    }
}

impl Backend for Fake {
    fn open(&mut self, index: u32) -> Result<Opened, String> {
        self.opened.lock().unwrap().push(index);
        match self.feeds[index as usize].clone() {
            Feed::Bytes(data, then) => Ok(Opened::Stream(Box::new(Steps { data, at: 0, then }))),
            Feed::Storage(_) => Ok(Opened::Storage),
        }
    }

    fn write_storage(
        &mut self,
        index: u32,
        path: &Path,
        created: &mut dyn FnMut(),
    ) -> Result<(), String> {
        let Feed::Storage(bytes) = &self.feeds[index as usize] else {
            return Err("not a storage item".into());
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        created();
        std::io::Write::write_all(&mut file, bytes).map_err(|error| error.to_string())
    }
}

pub(super) fn file(index: u32, name: &str, size: Option<u64>) -> VirtualEntry {
    VirtualEntry {
        index,
        name: name.to_owned(),
        path: Ok(name.split('\\').collect::<PathBuf>()),
        is_dir: false,
        size,
        modified: Some(time(u64::from(index))),
    }
}

pub(super) fn folder(index: u32, name: &str) -> VirtualEntry {
    VirtualEntry {
        is_dir: true,
        size: None,
        ..file(index, name, None)
    }
}

pub(super) fn refused(index: u32, name: &str, bad: BadName) -> VirtualEntry {
    VirtualEntry {
        path: Err(bad),
        ..file(index, name, Some(1))
    }
}

/// A paste over `feeds`, indexed like `entries`.
pub(super) struct Scene {
    pub paste: StreamPaste,
    pub opened: Arc<Mutex<Vec<u32>>>,
}

pub(super) fn scene_in(
    entries: Vec<VirtualEntry>,
    feeds: Vec<Feed>,
    (kind, stall): (SourceKind, Duration),
    archive: Option<PathBuf>,
) -> Scene {
    let opened = Arc::new(Mutex::new(Vec::new()));
    let fake = Fake {
        feeds,
        opened: Arc::clone(&opened),
    };
    let mut clipboard = VirtualClipboard::from_backend(entries, kind, move || Box::new(fake))
        .unwrap()
        .with_stall(stall);
    if let Some(archive) = archive {
        clipboard = clipboard.with_archive(archive);
    }
    Scene {
        paste: stream_paste(clipboard).unwrap(),
        opened,
    }
}

pub(super) fn scene(
    entries: Vec<VirtualEntry>,
    feeds: Vec<Feed>,
    kind_stall: (SourceKind, Duration),
) -> Scene {
    scene_in(entries, feeds, kind_stall, None)
}

pub(super) fn remote(entries: Vec<VirtualEntry>, feeds: Vec<Feed>) -> Scene {
    scene(
        entries,
        feeds,
        (SourceKind::RemoteSession, Duration::from_secs(5)),
    )
}

pub(super) fn location(path: &Path) -> Location {
    crate::startup::path_location(path).unwrap()
}

pub(super) fn plan_into(paste: &StreamPaste, target: &Location) -> OperationPlan {
    let existing = existing_at(&LocalFs::new(), &paste.items, target, &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items: paste.items.clone(),
        target: Some(target.clone()),
        existing,
        policy: ConflictPolicy::Ask,
        same_volume: false,
        protected: Vec::new(),
    })
}

/// Runs the paste into `target` the way the browser does: one worker, unlogged.
pub(super) fn run_into(
    paste: &StreamPaste,
    target: &Path,
    on_progress: &mut dyn FnMut(&Progress),
    cancel: &CancelToken,
) -> JobOutcome {
    let plan = plan_into(paste, &location(target));
    let mut log = (Operations::unlogged().log)(&plan).unwrap();
    run_job_parallel_with_sources(
        (&plan, &paste.sources),
        &[],
        &mut JobContext {
            ops: &LocalFs::new(),
            log: log.as_mut(),
            cancel,
            progress: on_progress,
        },
        1,
    )
}

/// Every name in `dir`, recursively, relative and with `/`.
pub(super) fn tree(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(dir).unwrap().to_string_lossy();
            found.push(relative.replace('\\', "/"));
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    found.sort();
    found
}

fn summary(paste: &StreamPaste) -> Vec<(String, u32, ItemKind, u64)> {
    paste
        .items
        .iter()
        .map(|item| {
            let path = item.location.path().display().into_owned();
            (path, item.depth, item.kind.clone(), item.size)
        })
        .collect()
}

#[test]
fn descriptor_entries_become_pre_order_items_with_implied_folders() {
    let scene = remote(
        vec![
            file(0, "top\\inner\\deep.txt", Some(4)),
            file(1, "readme.txt", Some(2)),
            folder(2, "top"),
            file(3, "top\\a.txt", Some(1)),
        ],
        Vec::new(),
    );
    let items = summary(&scene.paste);
    let expected = [
        ("/top", 0, ItemKind::Folder, 0),
        ("/top/inner", 1, ItemKind::Folder, 0),
        ("/top/inner/deep.txt", 2, ItemKind::File, 4),
        ("/top/a.txt", 1, ItemKind::File, 1),
        ("/readme.txt", 0, ItemKind::File, 2),
    ];
    let expected: Vec<_> = expected
        .iter()
        .map(|(path, depth, kind, size)| ((*path).to_owned(), *depth, kind.clone(), *size))
        .collect();
    assert_eq!(items, expected);
    assert!(!scene.paste.items_only, "every size is known");
    assert!(scene.paste.refused.is_empty());
}

#[test]
fn refused_and_repeated_names_are_skipped_with_their_reasons() {
    let scene = remote(
        vec![
            file(0, "ok.txt", Some(1)),
            refused(1, "..\\up.txt", BadName::Traversal),
            file(2, "OK.TXT", Some(1)),
            file(3, "clash", Some(1)),
            file(4, "clash\\below.txt", Some(1)),
            file(5, "nosize.bin", None),
        ],
        Vec::new(),
    );
    let names: Vec<_> = scene
        .paste
        .items
        .iter()
        .map(|item| item.location.path().display().into_owned())
        .collect();
    assert_eq!(names, ["/ok.txt", "/clash", "/nosize.bin"]);
    let refused: Vec<_> = scene
        .paste
        .refused
        .iter()
        .map(|item| (item.name.as_str(), item.reason.as_str()))
        .collect();
    let same = navo_l10n::tr("paste-virtual-same-name");
    assert_eq!(
        refused,
        [
            ("..\\up.txt", "The name leaves the target folder"),
            ("OK.TXT", same.as_str()),
            ("clash\\below.txt", same.as_str()),
        ]
    );
    assert!(scene.paste.items_only, "an unknown size shows item counts");
}
