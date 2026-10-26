use std::path::PathBuf;

pub(super) fn resolve() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(path) = windows::known(&windows::FOLDERID_Downloads).filter(|path| path.is_dir()) {
        return Some(path);
    }
    fallback(home(), "Downloads")
}

pub(super) fn resolve_desktop() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(path) = windows::known(&windows::FOLDERID_Desktop).filter(|path| path.is_dir()) {
        return Some(path);
    }
    fallback(home(), "Desktop")
}

fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn fallback(home: Option<PathBuf>, folder: &str) -> Option<PathBuf> {
    let home = home?;
    let inside = home.join(folder);
    Some(if inside.is_dir() { inside } else { home })
}

#[cfg(windows)]
mod windows {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    pub(super) use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_Downloads};
    use windows_sys::core::GUID;

    pub(super) fn known(folder: &GUID) -> Option<PathBuf> {
        let mut text = std::ptr::null_mut();
        // SAFETY: the API writes an owned, null-terminated UTF-16 allocation to text.
        let result =
            unsafe { SHGetKnownFolderPath(folder, 0, std::ptr::null_mut(), &raw mut text) };
        let path = if result >= 0 && !text.is_null() {
            let mut len = 0;
            // SAFETY: successful SHGetKnownFolderPath returns a null-terminated string.
            while unsafe { *text.add(len) } != 0 {
                len += 1;
            }
            // SAFETY: len spans the initialized string, excluding its terminator.
            let units = unsafe { std::slice::from_raw_parts(text, len) };
            Some(PathBuf::from(OsString::from_wide(units)))
        } else {
            None
        };
        // SAFETY: text is null or the allocation returned by SHGetKnownFolderPath.
        unsafe { CoTaskMemFree(text.cast()) };
        path
    }
}

#[cfg(test)]
mod tests {
    use super::fallback;

    #[test]
    fn downloads_folder_falls_back_to_home() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            fallback(Some(dir.path().into()), "Downloads"),
            Some(dir.path().into())
        );
        let downloads = dir.path().join("Downloads");
        std::fs::create_dir(&downloads).unwrap();
        assert_eq!(
            fallback(Some(dir.path().into()), "Downloads"),
            Some(downloads)
        );
        assert_eq!(fallback(None, "Downloads"), None);
        #[cfg(windows)]
        assert!(crate::crash::downloads_dir().unwrap().is_dir());
    }
}
