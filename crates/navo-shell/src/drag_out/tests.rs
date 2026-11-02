use std::path::PathBuf;

use super::*;

fn paths(items: &[&str]) -> Vec<PathBuf> {
    items.iter().map(PathBuf::from).collect()
}

#[test]
fn nothing_to_drag_is_rejected() {
    assert_eq!(same_folder(&[]), Err(DragError::Empty));
}

#[test]
fn items_in_one_folder_can_be_dragged() {
    let items = paths(&["/data/a.txt", "/data/b.txt", "/data/sub"]);
    assert_eq!(same_folder(&items), Ok(()));
}

#[test]
fn items_in_two_folders_are_mixed() {
    let items = paths(&["/data/a.txt", "/data/sub/b.txt"]);
    assert_eq!(same_folder(&items), Err(DragError::MixedFolders));
}

#[test]
fn dot_segments_name_the_same_folder() {
    let items = paths(&["/data/a.txt", "/data/./b.txt"]);
    assert_eq!(same_folder(&items), Ok(()));
}

#[test]
fn mixed_folders_are_refused_before_the_platform() {
    let items = paths(&["/data/a.txt", "/other/b.txt"]);
    assert_eq!(drag_files(0, &items), Err(DragError::MixedFolders));
}

#[cfg(unix)]
#[test]
fn unix_has_no_drag_out() {
    let items = paths(&["/data/a.txt", "/data/b.txt"]);
    assert_eq!(drag_files(0, &items), Err(DragError::Unsupported));
}
