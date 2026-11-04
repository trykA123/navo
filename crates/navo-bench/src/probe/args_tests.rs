use std::path::{Path, PathBuf};

use super::args::{build, verify_outputs};
use super::*;
use crate::fixture::{link_dir, move_dir};
use crate::guard::{GuardError, PathRole, ScratchRoot};

fn scratch() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, root)
}

fn folder_in(parent: &Path) -> tempfile::TempDir {
    tempfile::tempdir_in(parent).unwrap()
}

fn refused(root: &ScratchRoot, path: &Path) {
    let result = build(root, &[Arg::ReadOnly(PathFlag::Folder, path)]);
    assert_eq!(result, Err(ProbeError::BadReadOnly));
}

#[test]
fn a_read_only_path_that_is_or_holds_a_link_is_refused() {
    let (parent, root) = scratch();
    let target = folder_in(parent.path());
    let real = folder_in(parent.path());
    let child = folder_in(real.path());
    // A link inside the run root that points outward must not pass as "outside".
    let inner = root.path().join("outward");
    link_dir(&inner, target.path()).unwrap();
    refused(&root, &inner);
    // Outside the root: the folder itself is a link, or a link lies above it.
    let linked = parent.path().join("linked");
    link_dir(&linked, real.path()).unwrap();
    refused(&root, &linked);
    refused(&root, &linked.join(child.path().file_name().unwrap()));
    assert!(build(&root, &[Arg::ReadOnly(PathFlag::Folder, real.path())]).is_ok());
}

#[test]
fn a_read_only_path_in_a_protected_folder_or_a_missing_one_is_refused() {
    let (parent, root) = scratch();
    let exe = std::env::current_exe().unwrap().canonicalize().unwrap();
    let exe_folder = exe.parent().unwrap().to_path_buf();
    refused(&root, &exe_folder);
    refused(&root, &parent.path().join("missing"));
    let file = tempfile::NamedTempFile::new_in(parent.path()).unwrap();
    refused(&root, file.path());
}

#[test]
fn a_destination_parent_swapped_for_a_link_is_refused_at_the_next_check() {
    let (parent, root) = scratch();
    root.create_dir(Path::new("out")).unwrap();
    let args = [Arg::Path(PathFlag::Out, Path::new("out/b1.json"))];
    assert!(build(&root, &args).is_ok());
    let outward = folder_in(parent.path());
    let out = root.path().join("out");
    move_dir(&out, &parent.path().join("moved")).unwrap();
    link_dir(&out, outward.path()).unwrap();
    assert_eq!(
        build(&root, &args),
        Err(ProbeError::Guard(GuardError::ReparsePoint(
            PathRole::ScratchRoot
        )))
    );
}

#[test]
fn outputs_must_be_regular_files_inside_the_root_after_the_exit() {
    let (_parent, root) = scratch();
    root.create_dir(Path::new("out")).unwrap();
    let made = Path::new("out/b1.json");
    assert_eq!(
        verify_outputs(&root, &[made]),
        Err(ProbeError::OutputMissing)
    );
    drop(root.create_new(made).unwrap());
    assert_eq!(verify_outputs(&root, &[made]), Ok(()));
    root.create_dir(Path::new("out/dir")).unwrap();
    assert_eq!(
        verify_outputs(&root, &[Path::new("out/dir")]),
        Err(ProbeError::OutputMissing)
    );
    let escaping: PathBuf = PathBuf::from("../x");
    assert_eq!(
        verify_outputs(&root, &[escaping.as_path()]),
        Err(ProbeError::NotRelative)
    );
}
