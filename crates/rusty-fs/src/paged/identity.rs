use std::fs::{File, Metadata};

pub(super) fn is_replaced(same_identity: Option<bool>, held: &Metadata, named: &Metadata) -> bool {
    match same_identity {
        Some(same) => !same,
        None => held.len() != named.len() || held.modified().ok() != named.modified().ok(),
    }
}

#[cfg(unix)]
pub(super) fn same_identity(held: &File, named: &File) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    let (held, named) = (held.metadata().ok()?, named.metadata().ok()?);
    Some(held.dev() == named.dev() && held.ino() == named.ino())
}

#[cfg(windows)]
pub(super) fn same_identity(held: &File, named: &File) -> Option<bool> {
    use same_file::Handle;
    let held = Handle::from_file(held.try_clone().ok()?).ok()?;
    let named = Handle::from_file(named.try_clone().ok()?).ok()?;
    (held == named).then_some(true)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn same_identity(_: &File, _: &File) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime};

    use super::is_replaced;

    #[test]
    fn without_identity_a_different_size_or_time_means_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (one, two) = (dir.path().join("one.log"), dir.path().join("two.log"));
        fs::write(&one, b"first").unwrap();
        fs::write(&two, b"second file").unwrap();
        let held = fs::metadata(&one).unwrap();
        assert!(!is_replaced(None, &held, &fs::metadata(&one).unwrap()));
        assert!(is_replaced(None, &held, &fs::metadata(&two).unwrap()));
        fs::write(&two, b"fifth").unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&two)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(is_replaced(None, &held, &fs::metadata(&two).unwrap()));
    }

    #[test]
    fn a_known_identity_decides_over_size_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("one.log");
        fs::write(&path, b"first").unwrap();
        let held = fs::metadata(&path).unwrap();
        assert!(is_replaced(Some(false), &held, &held));
        fs::write(&path, b"first and more").unwrap();
        assert!(!is_replaced(
            Some(true),
            &held,
            &fs::metadata(&path).unwrap()
        ));
    }
}
