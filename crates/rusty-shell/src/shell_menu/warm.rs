use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{MenuPaths, MenuSource};

const SCRATCH_FOLDER: &str = "navo-shell-warm";
/// Warming removes its file within seconds; one this old was left by a crash.
const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// Steps that load shell extensions before the first real menu: open a menu, open one of its submenus, close it.
pub(super) enum Warm {
    Open {
        source: MenuSource,
        scratch: Option<Scratch>,
    },
    Fill {
        session: u64,
        path: Vec<usize>,
    },
    Close {
        session: u64,
        scratch: Option<Scratch>,
    },
}

/// A small file made only to read a file menu; it is removed when dropped.
pub(super) struct Scratch(PathBuf);

impl Scratch {
    pub(super) fn create(temp: &Path) -> std::io::Result<Self> {
        let folder = temp.join(SCRATCH_FOLDER);
        std::fs::create_dir_all(&folder)?;
        remove_stale(&folder, SystemTime::now());
        let file = folder.join(format!("menu-{}.txt", std::process::id()));
        std::fs::write(&file, b"")?;
        Ok(Self(file))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

fn remove_stale(folder: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let is_scratch = name
            .to_str()
            .is_some_and(|name| name.starts_with("menu-") && name.ends_with(".txt"));
        let modified = entry.metadata().and_then(|metadata| metadata.modified());
        let is_stale =
            modified.is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age > STALE_AFTER));
        if is_scratch && is_stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(folder) = self.0.parent() {
            let _ = std::fs::remove_dir(folder);
        }
    }
}

/// A folder menu, the folder's own background menu and a file menu, all in `temp`.
pub(super) fn first_steps(temp: &Path) -> Vec<Warm> {
    let open = |paths, scratch| Warm::Open {
        source: MenuSource {
            paths,
            extended: false,
        },
        scratch,
    };
    let mut steps = vec![
        open(MenuPaths::Items(vec![temp.to_path_buf()]), None),
        open(MenuPaths::Background(temp.to_path_buf()), None),
    ];
    if let Ok(scratch) = Scratch::create(temp) {
        steps.push(open(
            MenuPaths::Items(vec![scratch.path().to_path_buf()]),
            Some(scratch),
        ));
    }
    steps
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    use super::{MenuPaths, Scratch, Warm, first_steps};

    struct TestDir(PathBuf);

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp() -> TestDir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("navo-warm-test-{}-{id}", std::process::id());
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).expect("temp test folder");
        TestDir(path)
    }

    #[test]
    fn warming_reads_a_folder_its_background_and_a_file() {
        let temp = temp();
        let steps = first_steps(&temp.0);
        let paths: Vec<_> = steps
            .iter()
            .map(|step| match step {
                Warm::Open { source, scratch } => (source.paths.clone(), scratch.is_some()),
                Warm::Fill { .. } | Warm::Close { .. } => panic!("only opens come first"),
            })
            .collect();
        let has_scratch: Vec<_> = paths.iter().map(|(_, scratch)| *scratch).collect();
        assert_eq!(has_scratch, [false, false, true]);
        let paths: Vec<_> = paths.into_iter().map(|(paths, _)| paths).collect();
        assert_eq!(paths[0], MenuPaths::Items(vec![temp.0.clone()]));
        assert_eq!(paths[1], MenuPaths::Background(temp.0.clone()));
        let MenuPaths::Items(file) = &paths[2] else {
            panic!("the third step reads a file menu");
        };
        assert!(file[0].is_file());
    }

    #[test]
    fn the_scratch_file_and_its_folder_go_when_dropped() {
        let temp = temp();
        let scratch = Scratch::create(&temp.0).expect("scratch file");
        let file = scratch.path().to_path_buf();
        assert!(file.is_file());
        drop(scratch);
        assert!(!file.exists());
        assert!(!file.parent().expect("a folder").exists());
    }

    #[test]
    fn dropping_unrun_steps_removes_the_scratch_file() {
        let temp = temp();
        let steps = first_steps(&temp.0);
        let folder = temp.0.join(super::SCRATCH_FOLDER);
        assert!(folder.exists());
        drop(steps);
        assert!(!folder.exists());
    }

    #[test]
    fn a_file_left_by_a_crash_is_reused_and_removed() {
        let temp = temp();
        let first = Scratch::create(&temp.0).expect("scratch file");
        let file = first.path().to_path_buf();
        std::mem::forget(first);
        let again = Scratch::create(&temp.0).expect("scratch file again");
        assert_eq!(again.path(), file);
        drop(again);
        assert!(!file.exists());
    }

    fn file_aged(folder: &std::path::Path, name: &str, age: Duration) -> PathBuf {
        let path = folder.join(name);
        let file = std::fs::File::create(&path).expect("test file");
        let modified = SystemTime::now() - age;
        file.set_modified(modified).expect("set the file time");
        path
    }

    #[test]
    fn a_new_scratch_removes_files_older_than_an_hour_left_by_other_runs() {
        let temp = temp();
        let folder = temp.0.join(super::SCRATCH_FOLDER);
        std::fs::create_dir_all(&folder).expect("scratch folder");
        let hour = Duration::from_secs(60 * 60);
        let crashed = file_aged(&folder, "menu-1.txt", hour * 2);
        let running = file_aged(&folder, "menu-2.txt", hour / 2);
        let foreign = file_aged(&folder, "notes.txt", hour * 2);
        let scratch = Scratch::create(&temp.0).expect("scratch file");
        assert!(!crashed.exists());
        assert!(running.is_file());
        assert!(foreign.is_file());
        drop(scratch);
    }

    #[test]
    fn a_folder_with_other_files_is_kept() {
        let temp = temp();
        let scratch = Scratch::create(&temp.0).expect("scratch file");
        let folder = scratch.path().parent().expect("a folder").to_path_buf();
        let other = folder.join("someone-else.txt");
        std::fs::write(&other, b"keep").expect("other file");
        drop(scratch);
        assert!(other.is_file());
    }
}
