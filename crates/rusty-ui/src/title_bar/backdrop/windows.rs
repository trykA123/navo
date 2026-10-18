pub(super) fn windows_build() -> Option<u32> {
    Some(windows_version::OsVersion::current().build)
}
