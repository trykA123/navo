use std::collections::{BTreeMap, BTreeSet};

use super::*;

const NAVO: &[&str] = &["navo-index.exe", "navo.exe"];
const PID: u32 = 77;

#[derive(Default)]
struct Fake {
    files: BTreeSet<PathBuf>,
    folders: BTreeSet<PathBuf>,
    refused_deletes: BTreeMap<PathBuf, u32>,
    refused_renames: BTreeSet<PathBuf>,
    refuse_schedule: bool,
    scheduled: Vec<PathBuf>,
    deletes: u32,
}

fn parent() -> PathBuf {
    PathBuf::from("pf/Navo")
}

fn index() -> PathBuf {
    parent().join("Index")
}

impl Fake {
    fn with(names: &[&str]) -> Self {
        let mut fake = Self::default();
        fake.folders.extend([parent(), index()]);
        fake.files
            .extend(names.iter().map(|name| index().join(name)));
        fake
    }

    fn refuse(&mut self, name: &str, times: u32) {
        self.refused_deletes.insert(index().join(name), times);
    }

    fn children(&self, folder: &Path) -> Vec<PathBuf> {
        self.files
            .iter()
            .chain(&self.folders)
            .filter(|path| path.parent() == Some(folder))
            .cloned()
            .collect()
    }
}

impl Files for Fake {
    fn list(&mut self, folder: &Path) -> io::Result<Vec<Entry>> {
        if !self.folders.contains(folder) {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(self
            .children(folder)
            .into_iter()
            .map(|path| Entry {
                is_file: self.files.contains(&path),
                path,
            })
            .collect())
    }
    fn delete(&mut self, path: &Path) -> io::Result<()> {
        self.deletes += 1;
        if let Some(left) = self.refused_deletes.get_mut(path)
            && *left > 0
        {
            *left -= 1;
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if self.files.remove(path) {
            Ok(())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        if self.refused_renames.contains(from) || self.files.contains(to) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.files.remove(from);
        self.files.insert(to.to_path_buf());
        Ok(())
    }
    fn schedule(&mut self, path: &Path) -> io::Result<()> {
        if self.refuse_schedule {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.scheduled.push(path.to_path_buf());
        Ok(())
    }
    fn remove_folder(&mut self, path: &Path) -> io::Result<()> {
        if !self.children(path).is_empty() {
            return Err(io::ErrorKind::DirectoryNotEmpty.into());
        }
        self.folders.remove(path);
        Ok(())
    }
}

fn limits() -> Limits {
    Limits {
        tries: 5,
        delay: Duration::ZERO,
        deadline: Instant::now() + Duration::from_secs(60),
        process: PID,
    }
}

fn clean_navo(fake: &mut Fake) -> io::Result<Cleanup> {
    let (parent, index) = (parent(), index());
    let folders = Folders {
        parent: &parent,
        index: &index,
    };
    clean(fake, folders, NAVO, &limits())
}

#[test]
fn removal_deletes_both_images_their_copies_and_both_folders() {
    let mut fake = Fake::with(&[
        "navo.exe",
        "navo-index.exe",
        "navo.exe.12.old",
        "navo.exe.4.tmp",
        "navo-index.exe.3.tmp",
        "navo-index.exe.9.old",
    ]);
    let cleanup = clean_navo(&mut fake).unwrap();
    assert!(cleanup.is_complete(), "{cleanup:?}");
    assert!(fake.files.is_empty(), "{:?}", fake.files);
    assert!(fake.folders.is_empty(), "{:?}", fake.folders);
    assert!(fake.scheduled.is_empty());
}

#[test]
fn removal_keeps_and_lists_unknown_files_and_their_folders_unscheduled() {
    let unknown = [
        "notes.txt",
        "navo.exe.old",
        "navo.exe.abc.old",
        "navo.exe.12.bak",
        "ferrite.exe",
    ];
    let mut names = vec!["navo.exe"];
    names.extend(unknown);
    let mut fake = Fake::with(&names);
    let cleanup = clean_navo(&mut fake).unwrap();
    let expected: BTreeSet<_> = unknown.iter().map(|name| index().join(name)).collect();
    assert_eq!(
        cleanup.unknown.iter().cloned().collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(fake.files, expected);
    assert!(fake.folders.contains(&index()) && fake.folders.contains(&parent()));
    assert!(fake.scheduled.is_empty(), "{:?}", fake.scheduled);
    assert!(!cleanup.is_complete());
}

#[test]
fn removal_never_deletes_a_folder_named_like_an_image() {
    let mut fake = Fake::with(&[]);
    fake.folders.insert(index().join("navo.exe"));
    let cleanup = clean_navo(&mut fake).unwrap();
    assert_eq!(cleanup.unknown, [index().join("navo.exe")]);
    assert!(fake.folders.contains(&index().join("navo.exe")));
}

#[test]
fn removal_retries_a_mapped_image_before_it_falls_back() {
    let mut fake = Fake::with(&["navo-index.exe"]);
    fake.refuse("navo-index.exe", 3);
    let cleanup = clean_navo(&mut fake).unwrap();
    assert!(cleanup.is_complete());
    assert_eq!(fake.deletes, 4);
}

#[test]
fn removal_renames_a_refused_image_and_schedules_it_before_its_folders() {
    let mut fake = Fake::with(&["navo-index.exe", "navo.exe"]);
    fake.refuse("navo-index.exe", u32::MAX);
    let cleanup = clean_navo(&mut fake).unwrap();
    let renamed = index().join(format!("navo-index.exe.{PID}.old"));
    assert_eq!(fake.scheduled, [renamed.clone(), index(), parent()]);
    assert_eq!(cleanup.scheduled, fake.scheduled);
    assert_eq!(fake.files, BTreeSet::from([renamed]));
    assert!(cleanup.unknown.is_empty());
}

#[test]
fn removal_schedules_the_original_path_when_the_rename_is_refused() {
    let mut fake = Fake::with(&["navo.exe"]);
    fake.refuse("navo.exe", u32::MAX);
    fake.refused_renames.insert(index().join("navo.exe"));
    clean_navo(&mut fake).unwrap();
    assert_eq!(
        fake.scheduled,
        [index().join("navo.exe"), index(), parent()]
    );
}

#[test]
fn removal_never_renames_over_an_earlier_renamed_copy() {
    let mut fake = Fake::with(&["navo.exe", "navo.exe.5.tmp"]);
    fake.refuse("navo.exe", u32::MAX);
    fake.refuse("navo.exe.5.tmp", u32::MAX);
    clean_navo(&mut fake).unwrap();
    let renamed = index().join(format!("navo.exe.{PID}.old"));
    assert_eq!(fake.files.len(), 2, "{:?}", fake.files);
    assert_eq!(fake.scheduled.len(), 4, "{:?}", fake.scheduled);
    assert!(fake.scheduled.contains(&renamed));
    assert_eq!(fake.scheduled[2..], [index(), parent()]);
}

#[test]
fn removal_fails_when_a_schedule_is_refused() {
    let mut fake = Fake::with(&["navo.exe"]);
    fake.refuse("navo.exe", u32::MAX);
    fake.refuse_schedule = true;
    let error = clean_navo(&mut fake).unwrap_err();
    assert!(error.to_string().contains("restart"), "{error}");
}

#[test]
fn removal_stops_retrying_once_the_deadline_has_passed() {
    let mut fake = Fake::with(&["navo.exe"]);
    fake.refuse("navo.exe", u32::MAX);
    let (parent, index) = (parent(), index());
    let late = Limits {
        deadline: Instant::now(),
        ..limits()
    };
    let folders = Folders {
        parent: &parent,
        index: &index,
    };
    let cleanup = clean(&mut fake, folders, NAVO, &late).unwrap();
    assert_eq!(fake.deletes, 1);
    assert_eq!(cleanup.scheduled.len(), 3);
}

#[test]
fn removal_keeps_a_parent_folder_that_holds_other_entries() {
    let mut fake = Fake::with(&["navo.exe"]);
    fake.files.insert(parent().join("readme.txt"));
    let cleanup = clean_navo(&mut fake).unwrap();
    assert!(!fake.folders.contains(&index()));
    assert!(fake.folders.contains(&parent()));
    assert_eq!(cleanup.unknown, [parent().join("readme.txt")]);
}

#[test]
fn removal_of_ferrite_deletes_only_its_own_image() {
    let mut fake = Fake::with(&["ferrite.exe", "ferrite.exe.2.old", "navo.exe"]);
    let (parent, index) = (parent(), index());
    let folders = Folders {
        parent: &parent,
        index: &index,
    };
    let cleanup = clean(&mut fake, folders, &["ferrite.exe"], &limits()).unwrap();
    assert_eq!(fake.files, BTreeSet::from([index.join("navo.exe")]));
    assert_eq!(cleanup.unknown, [index.join("navo.exe")]);
}

#[test]
fn removal_of_an_absent_folder_is_complete() {
    let mut fake = Fake::default();
    let cleanup = clean_navo(&mut fake).unwrap();
    assert!(cleanup.is_complete());
}

#[test]
fn removable_names_are_the_images_and_their_numbered_copies_only() {
    let images = super::super::super::install_plan::NAVO.removable;
    for (name, image) in [
        ("navo.exe", Some("navo.exe")),
        ("navo-index.exe", Some("navo-index.exe")),
        ("navo.exe.12.old", Some("navo.exe")),
        ("navo-index.exe.4.tmp", Some("navo-index.exe")),
        ("NAVO.EXE", Some("navo.exe")),
        ("navo.exe.old", None),
        ("navo.exe.1a.tmp", None),
        ("navo.exe.3.bak", None),
        ("navo-index.exe.exe", None),
        ("ferrite.exe", None),
    ] {
        assert_eq!(removable_image(images, name), image, "{name}");
    }
}

#[test]
fn removal_reports_what_both_products_left() {
    let mut left = Cleanup {
        scheduled: vec![index()],
        unknown: Vec::new(),
    };
    left.extend(Cleanup {
        scheduled: vec![parent()],
        unknown: vec![index().join("notes.txt")],
    });
    assert_eq!(left.scheduled, [index(), parent()]);
    assert_eq!(left.unknown, [index().join("notes.txt")]);
}
