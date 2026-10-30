use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use super::*;
use crate::{FileIdentityKind, FileStamp, MoveProof, ProviderOps, RenameSkip, VerifiedRename};

struct Moved {
    dir: TempDir,
    fs: LocalFs,
}

impl Moved {
    fn new() -> Self {
        let (dir, _) = temp();
        for folder in ["src", "dst", "aside"] {
            fs::create_dir(dir.path().join(folder)).unwrap();
        }
        fs::write(dir.path().join("dst/a.txt"), "alpha").unwrap();
        Self {
            dir,
            fs: LocalFs::new(),
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn at(&self, relative: &str) -> Location {
        location_of(&self.path(relative))
    }

    fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.path(relative)).ok()
    }

    fn proof(&self, relative: &str) -> MoveProof {
        let location = self.at(relative);
        let entry = self.fs.metadata(&location).unwrap();
        let stamp = (entry.kind == EntryKind::File).then_some(FileStamp {
            size: entry.size,
            modified_ms: entry.modified_ms,
        });
        MoveProof::durable(self.fs.file_identity(&location).unwrap().unwrap(), stamp).unwrap()
    }

    fn back(&self, relative: &str, proof: &MoveProof) -> Result<VerifiedRename, FsError> {
        self.fs.rename_verified(
            &self.at(&format!("dst/{relative}")),
            &self.at(&format!("src/{relative}")),
            proof,
        )
    }

    fn step(&self, relative: &str) -> rusty_core::Step {
        rusty_core::Step::Rename {
            source: self.at(&format!("src/{relative}")),
            target: self.at(&format!("dst/{relative}")),
        }
    }

    fn swap_in_replacement(&self) -> impl FnOnce() + 'static {
        let (dst, aside) = (self.path("dst/a.txt"), self.path("aside/a.txt"));
        move || {
            fs::rename(&dst, aside).unwrap();
            fs::write(dst, "replacement").unwrap();
        }
    }
}

fn skipped(skip: RenameSkip) -> Result<VerifiedRename, FsError> {
    Ok(VerifiedRename::Skipped(skip))
}

#[test]
fn the_same_file_moves_back_to_its_origin() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    assert_eq!(moved.back("a.txt", &proof), Ok(VerifiedRename::Renamed));
    assert_eq!(
        (moved.read("src/a.txt"), moved.read("dst/a.txt")),
        (Some("alpha".to_owned()), None)
    );
}

#[test]
fn a_folder_moves_back_with_a_child_changed_inside_it() {
    let moved = Moved::new();
    fs::create_dir(moved.path("dst/sub")).unwrap();
    fs::write(moved.path("dst/sub/c.txt"), "charlie").unwrap();
    let proof = moved.proof("dst/sub");
    fs::write(moved.path("dst/sub/c.txt"), "charlie, edited").unwrap();
    assert_eq!(moved.back("sub", &proof), Ok(VerifiedRename::Renamed));
    assert_eq!(
        moved.read("src/sub/c.txt").as_deref(),
        Some("charlie, edited")
    );
}

#[test]
fn a_replacement_found_at_the_check_never_moves() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    moved.swap_in_replacement()();
    assert_eq!(moved.back("a.txt", &proof), skipped(RenameSkip::Replaced));
    assert_eq!(
        (moved.read("dst/a.txt"), moved.read("src/a.txt")),
        (Some("replacement".to_owned()), None)
    );
}

#[test]
fn a_replacement_swapped_in_after_the_check_never_moves() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    crate::local::verified::before_rename(moved.swap_in_replacement());
    let result = moved.back("a.txt", &proof);
    assert_eq!(moved.read("dst/a.txt").as_deref(), Some("replacement"));
    if cfg!(windows) {
        assert_eq!(result, Ok(VerifiedRename::Renamed));
        assert_eq!(moved.read("src/a.txt").as_deref(), Some("alpha"));
    } else {
        assert_eq!(result, skipped(RenameSkip::Replaced));
        assert_eq!(moved.read("src/a.txt"), None);
        assert_eq!(moved.read("aside/a.txt").as_deref(), Some("alpha"));
    }
}

#[test]
fn an_origin_taken_after_the_check_is_never_overwritten() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    let origin = moved.path("src/a.txt");
    crate::local::verified::before_rename(move || fs::write(origin, "made later").unwrap());
    assert_eq!(
        moved.back("a.txt", &proof),
        skipped(RenameSkip::OriginOccupied)
    );
    assert_eq!(
        (moved.read("src/a.txt"), moved.read("dst/a.txt")),
        (Some("made later".to_owned()), Some("alpha".to_owned()))
    );
}

#[test]
fn an_occupied_origin_is_skipped() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    fs::write(moved.path("src/a.txt"), "made later").unwrap();
    assert_eq!(
        moved.back("a.txt", &proof),
        skipped(RenameSkip::OriginOccupied)
    );
    assert_eq!(moved.read("src/a.txt").as_deref(), Some("made later"));
}

#[test]
fn a_change_to_only_the_modified_time_is_detected() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    let later = SystemTime::now() + Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(moved.path("dst/a.txt"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(moved.back("a.txt", &proof), skipped(RenameSkip::Changed));
    assert_eq!(moved.read("dst/a.txt").as_deref(), Some("alpha"));
}

#[test]
fn a_change_of_size_is_detected() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    fs::write(moved.path("dst/a.txt"), "alpha, longer").unwrap();
    assert_eq!(moved.back("a.txt", &proof), skipped(RenameSkip::Changed));
}

#[test]
fn a_destination_that_is_gone_is_skipped() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    fs::remove_file(moved.path("dst/a.txt")).unwrap();
    assert_eq!(moved.back("a.txt", &proof), skipped(RenameSkip::Missing));
}

#[test]
fn a_fingerprint_is_never_a_durable_proof() {
    for folder in [false, true] {
        let fingerprint = crate::FileIdentity::fingerprint(5, Some(7), folder);
        assert!(matches!(
            fingerprint.kind,
            FileIdentityKind::Fingerprint { .. }
        ));
        assert_eq!(MoveProof::durable(fingerprint, None), None);
    }
}

#[cfg(windows)]
#[test]
fn a_read_only_file_moves_back_through_its_handle() {
    let moved = Moved::new();
    let path = moved.path("dst/a.txt");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let proof = moved.proof("dst/a.txt");
    assert_eq!(moved.back("a.txt", &proof), Ok(VerifiedRename::Renamed));
    assert_eq!(moved.read("src/a.txt").as_deref(), Some("alpha"));
}

#[cfg(unix)]
#[test]
fn a_replacement_that_cannot_go_back_is_unresolved_and_nothing_is_lost() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    crate::local::verified::before_rename(moved.swap_in_replacement());
    let taken = moved.path("dst/a.txt");
    crate::local::verified::before_move_back(move || fs::write(taken, "made later").unwrap());
    assert_eq!(
        moved.back("a.txt", &proof),
        Ok(VerifiedRename::Unresolved {
            at: moved.at("src/a.txt")
        })
    );
    assert_eq!(
        present(&moved, &["src/a.txt", "dst/a.txt", "aside/a.txt"]),
        ["replacement", "made later", "alpha"].map(|text| Some(text.to_owned()))
    );
}

#[cfg(unix)]
#[test]
fn an_unresolved_move_back_is_a_failed_undo_step_naming_where_the_item_is() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    let log = Recorded(Some(proof));
    crate::local::verified::before_rename(moved.swap_in_replacement());
    let taken = moved.path("dst/a.txt");
    crate::local::verified::before_move_back(move || fs::write(taken, "made later").unwrap());
    let failure = crate::undo_move(&moved.fs, &log, (0, &moved.step("a.txt"))).unwrap_err();
    assert_eq!(
        (failure.kind, failure.location),
        (
            FsErrorKind::IdentityUnavailable,
            Some(moved.at("src/a.txt"))
        )
    );
}

#[test]
fn a_file_proof_without_a_stamp_is_refused_and_the_file_stays() {
    let moved = Moved::new();
    let identity = moved
        .fs
        .file_identity(&moved.at("dst/a.txt"))
        .unwrap()
        .unwrap();
    let proof = MoveProof::durable(identity, None).unwrap();
    fs::write(moved.path("dst/a.txt"), "alpha, edited").unwrap();
    let failure = moved.back("a.txt", &proof).unwrap_err();
    assert_eq!(failure.kind, FsErrorKind::IdentityUnavailable);
    assert_eq!(
        present(&moved, &["dst/a.txt", "src/a.txt"]),
        [Some("alpha, edited".to_owned()), None]
    );
}

#[test]
fn an_item_already_back_at_its_origin_counts_as_undone() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    fs::rename(moved.path("dst/a.txt"), moved.path("src/a.txt")).unwrap();
    let log = Recorded(Some(proof));
    assert_eq!(
        crate::undo_move(&moved.fs, &log, (0, &moved.step("a.txt"))),
        Ok(crate::StepOutcome::Done)
    );
}

#[cfg(windows)]
#[test]
fn a_file_open_for_writing_is_in_use_and_stays() {
    let moved = Moved::new();
    let proof = moved.proof("dst/a.txt");
    let writer = fs::File::options()
        .write(true)
        .open(moved.path("dst/a.txt"))
        .unwrap();
    assert_eq!(moved.back("a.txt", &proof), skipped(RenameSkip::InUse));
    let log = Recorded(Some(proof));
    let failure = crate::undo_move(&moved.fs, &log, (0, &moved.step("a.txt"))).unwrap_err();
    assert_eq!(failure.kind, FsErrorKind::SharingViolation);
    drop(writer);
    assert_eq!(moved.read("dst/a.txt").as_deref(), Some("alpha"));
    assert_eq!(moved.back("a.txt", &proof), Ok(VerifiedRename::Renamed));
}

fn present<const N: usize>(moved: &Moved, paths: &[&str; N]) -> [Option<String>; N] {
    paths.map(|path| moved.read(path))
}

struct Recorded(Option<MoveProof>);

impl crate::JobLog for Recorded {
    fn rename_identity(&self, _: u32) -> Result<Option<crate::FileIdentity>, FsError> {
        Ok(self.0.map(|proof| proof.identity()))
    }

    fn rename_stamp(&self, _: u32) -> Result<Option<FileStamp>, FsError> {
        Ok(self.0.and_then(|proof| proof.stamp()))
    }

    fn step(&mut self, _: u32, _: crate::StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, _: u32, _: crate::StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _: &crate::HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}
