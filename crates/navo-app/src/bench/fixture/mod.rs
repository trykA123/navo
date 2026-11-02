mod names;

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use names::Rng;

pub(super) const SEED: u64 = 0x4E41_564F_4245_4E43;
pub(super) const DEEP_FILE: &str = "brake_controller_main.c";
pub(super) const UNICODE_FILE: &str = "Größenprüfung_日本語_データ.a2l";
pub(super) const INCREMENTAL: &str = "incremental";
pub(super) const HUGE: [&str; 2] = ["huge-500k", "huge-100k"];
const SENTINELS: [&str; 2] = [DEEP_FILE, UNICODE_FILE];
const MAX_DEPTH: usize = 12;
const CHUNK: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Names {
    Tree(u64),
    Huge(u64),
    Victims,
    Sentinels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Folder {
    path: PathBuf,
    depth: usize,
    files: usize,
    names: Names,
}

impl Folder {
    fn name(&self, index: usize) -> String {
        match self.names {
            Names::Tree(seed) => names::file_name(seed, index, self.depth),
            Names::Huge(seed) => names::huge_name(seed, index),
            Names::Victims => victim_name(index),
            Names::Sentinels => SENTINELS[index].to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Plan {
    folders: Vec<Folder>,
    pub(super) files: usize,
    pub(super) huge: [usize; 2],
    pub(super) operations: usize,
}

pub(super) fn victim_name(index: usize) -> String {
    format!("victim-{index:05}.c")
}

impl Plan {
    pub(super) fn new(files: usize) -> Self {
        let mut rng = Rng::new(SEED);
        let huge = [(files / 4).min(500_000), (files / 20).min(100_000)];
        let operations = (files / 10).min(10_000);
        let victims = operations * 3 / 10 * 2;
        let mut folders = deep_chain();
        for (name, count) in HUGE.iter().zip(huge) {
            folders.push(Folder {
                path: PathBuf::from(name),
                depth: 1,
                files: count,
                names: Names::Huge(rng.next()),
            });
        }
        folders.push(Folder {
            path: PathBuf::from(INCREMENTAL),
            depth: 1,
            files: victims,
            names: Names::Victims,
        });
        let used: usize = folders.iter().map(|folder| folder.files).sum();
        folders.extend(tree(files.saturating_sub(used), &mut rng));
        Self {
            files: folders.iter().map(|folder| folder.files).sum(),
            folders,
            huge,
            operations,
        }
    }

    pub(super) fn folders(&self) -> usize {
        self.folders.len()
    }
}

fn deep_chain() -> Vec<Folder> {
    let mut path = PathBuf::from("deep");
    let mut chain = Vec::new();
    for depth in 1..=MAX_DEPTH {
        if depth > 1 {
            path.push(format!("level-{depth}"));
        }
        chain.push(Folder {
            path: path.clone(),
            depth,
            files: if depth == MAX_DEPTH {
                SENTINELS.len()
            } else {
                0
            },
            names: Names::Sentinels,
        });
    }
    chain
}

fn tree(mut remaining: usize, rng: &mut Rng) -> Vec<Folder> {
    let mut folders = vec![Folder {
        path: PathBuf::from("tree"),
        depth: 1,
        files: 0,
        names: Names::Tree(rng.next()),
    }];
    while remaining > 0 {
        let parent = &folders[pick_parent(folders.len(), rng)];
        let parent = if parent.depth >= MAX_DEPTH {
            &folders[0]
        } else {
            parent
        };
        let count = if rng.below(100) < 3 {
            200 + rng.below(800)
        } else {
            rng.below(80)
        }
        .min(remaining);
        remaining -= count;
        let folder = Folder {
            path: parent.path.join(names::folder_name(rng, folders.len())),
            depth: parent.depth + 1,
            files: count,
            names: Names::Tree(rng.next()),
        };
        folders.push(folder);
    }
    folders
}

fn pick_parent(count: usize, rng: &mut Rng) -> usize {
    let start = if rng.below(10) < 6 {
        count.saturating_sub(32)
    } else {
        0
    };
    start + rng.below(count - start)
}

pub(super) fn create(root: &Path, plan: &Plan) -> io::Result<()> {
    for folder in &plan.folders {
        let path = root.join(&folder.path);
        fs::create_dir_all(&path).map_err(|error| context("creating folder", &path, &error))?;
    }
    let units: Vec<(&Folder, usize)> = plan
        .folders
        .iter()
        .flat_map(|folder| (0..folder.files).step_by(CHUNK).map(move |at| (folder, at)))
        .collect();
    let work = Work {
        units,
        next: AtomicUsize::new(0),
        failed: AtomicBool::new(false),
    };
    let workers = std::thread::available_parallelism().map_or(4, |count| count.get().min(16));
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| scope.spawn(|| work.drain(root)))
            .collect();
        handles.into_iter().try_for_each(|handle| {
            handle
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("a file worker stopped")))
        })
    })
}

struct Work<'a> {
    units: Vec<(&'a Folder, usize)>,
    next: AtomicUsize,
    failed: AtomicBool,
}

impl Work<'_> {
    fn drain(&self, root: &Path) -> io::Result<()> {
        while !self.failed.load(Ordering::Relaxed) {
            let Some(&(folder, at)) = self.units.get(self.next.fetch_add(1, Ordering::Relaxed))
            else {
                return Ok(());
            };
            create_files(root, folder, at)
                .inspect_err(|_| self.failed.store(true, Ordering::Relaxed))?;
        }
        Ok(())
    }
}

fn create_files(root: &Path, folder: &Folder, at: usize) -> io::Result<()> {
    let path = root.join(&folder.path);
    for index in at..(at + CHUNK).min(folder.files) {
        let file = path.join(folder.name(index));
        File::options()
            .write(true)
            .create_new(true)
            .open(&file)
            .map_err(|error| context("creating file", &file, &error))?;
    }
    Ok(())
}

pub(super) fn context(doing: &str, path: &Path, error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{doing} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests;
