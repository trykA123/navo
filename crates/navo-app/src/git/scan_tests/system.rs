use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};

use navo_core::Location;
use navo_fs::DriveClass;

use super::root_class_with;

fn class_with(class: DriveClass, root: &Path, listing: &Location) -> (DriveClass, usize) {
    let reads = Cell::new(0);
    let mount = |_: &Path| -> io::Result<PathBuf> {
        reads.set(reads.get() + 1);
        Err(io::Error::other("no volume"))
    };
    (
        root_class_with(root, listing, mount, |_| class),
        reads.get(),
    )
}

#[test]
fn only_a_fixed_listing_class_reads_the_root_volume() {
    let dir = tempfile::tempdir().unwrap();
    let listing = Location::new(dir.path().to_str().unwrap()).unwrap();
    for class in [
        DriveClass::Network,
        DriveClass::Removable,
        DriveClass::Unknown,
    ] {
        let (found, reads) = class_with(class, dir.path(), &listing);
        assert_eq!(found, class, "a non-fixed class is returned as it is");
        assert_eq!(
            reads, 0,
            "{class:?}: a network or removable root must not block the scanner on volume reads"
        );
    }
    for class in [DriveClass::Ssd, DriveClass::Hdd] {
        let (found, reads) = class_with(class, dir.path(), &listing);
        assert_eq!(
            found,
            DriveClass::Unknown,
            "a failed volume read is unknown"
        );
        assert_eq!(reads, 1, "{class:?}: a fixed class is proven on its volume");
    }
}
