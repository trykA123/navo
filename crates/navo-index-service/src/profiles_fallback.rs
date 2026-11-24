//! Fail-closed profile scope (D126, AT15): when the installing user's scope cannot be read, every
//! folder under the profiles root is hidden except `Public` and the own profile when it is known.
//! The result is an ordinary `ProfileScope`, so hits and totals use the usual hiding path.

use super::{PROFILE_LIST, ProfileScope, ProfileSource, both_forms, local_folder, scope_from};
use crate::service_log::Log;

/// The profiles folder under the system drive when the registry root is unusable.
const USERS: &str = "Users";

/// Reads the scope for `sid`; when that fails, logs the reason once and returns the fallback
/// scope. `system_drive` is like `C:` and may be missing.
pub(super) fn scope_or_fallback(
    sid: &str,
    source: &impl ProfileSource,
    system_drive: Option<&str>,
    log: &Log,
) -> ProfileScope {
    scope_from(sid, source).unwrap_or_else(|reason| {
        log.line(format_args!(
            "profiles unresolved: {reason}; other users' folders are hidden"
        ));
        fallback_scope(sid, source, system_drive)
    })
}

/// Guards the registry profiles root, else `\Users` on `system_drive`, else `\Users` on every
/// drive letter; keeps `Public` and the own profile when its registry path is local.
pub(super) fn fallback_scope(
    sid: &str,
    source: &impl ProfileSource,
    system_drive: Option<&str>,
) -> ProfileScope {
    let users = source
        .text(&format!(r"{PROFILE_LIST}\{sid}"), "ProfileImagePath")
        .and_then(|path| both_forms(source, &path).ok())
        .unwrap_or_default();
    let roots = source
        .text(PROFILE_LIST, "ProfilesDirectory")
        .and_then(|root| both_forms(source, &root).ok())
        .unwrap_or_else(|| {
            users_folders(system_drive)
                .into_iter()
                .flat_map(|folder| both_forms(source, &folder).unwrap_or_else(|_| vec![folder]))
                .collect()
        });
    ProfileScope::build(&roots, &users, Vec::new())
}

/// `X:\Users` on the system drive; on every drive letter when the drive is missing or unusable.
fn users_folders(system_drive: Option<&str>) -> Vec<String> {
    let on_system = system_drive.and_then(|drive| {
        let drive = drive.trim().trim_end_matches(['\\', '/']);
        local_folder(&format!(r"{drive}\{USERS}")).ok()
    });
    match on_system {
        Some(folder) => vec![folder],
        None => (b'A'..=b'Z')
            .map(|letter| format!(r"{}:\{USERS}", char::from(letter)))
            .collect(),
    }
}

#[cfg(test)]
#[path = "profiles_fallback_tests.rs"]
mod tests;
