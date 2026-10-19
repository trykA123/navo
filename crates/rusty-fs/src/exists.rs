use rusty_core::FsError;

#[cfg(windows)]
pub(crate) const ALREADY_EXISTS_CODES: &[i32] = &[183, 80];
#[cfg(not(windows))]
pub(crate) const ALREADY_EXISTS_CODES: &[i32] = &[17];

pub(crate) fn already_exists(error: &FsError) -> bool {
    error
        .os_code
        .is_some_and(|code| ALREADY_EXISTS_CODES.contains(&code))
}
