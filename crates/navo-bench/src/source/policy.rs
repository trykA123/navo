use std::path::{Path, PathBuf};

use super::SourceError;

pub(super) struct FolderPolicy {
    pub(super) windows: Option<PathBuf>,
    pub(super) program_files: Option<PathBuf>,
    pub(super) program_files_x86: Option<PathBuf>,
    pub(super) program_data: Option<PathBuf>,
    pub(super) profile: PathBuf,
    pub(super) profiles: Option<PathBuf>,
    /// The report is written here, so the chosen folder may neither hold it nor lie in it.
    pub(super) exe_folder: PathBuf,
    pub(super) scratch_root: PathBuf,
}

impl FolderPolicy {
    #[cfg(unix)]
    pub(super) fn load(scratch_root: &Path) -> Result<Self, SourceError> {
        let profile = std::env::var_os("HOME").ok_or(SourceError::FolderLookup)?;
        Ok(Self {
            windows: None,
            program_files: None,
            program_files_x86: None,
            program_data: None,
            profile: PathBuf::from(profile),
            profiles: None,
            exe_folder: exe_folder()?,
            scratch_root: canonical(scratch_root)?,
        })
    }

    #[cfg(windows)]
    pub(super) fn load(scratch_root: &Path) -> Result<Self, SourceError> {
        use windows::Win32::UI::Shell::{
            FOLDERID_Profile, FOLDERID_ProgramData, FOLDERID_ProgramFiles,
            FOLDERID_ProgramFilesX86, FOLDERID_UserProfiles, FOLDERID_Windows,
        };
        Ok(Self {
            windows: Some(known(&FOLDERID_Windows)?),
            program_files: Some(known(&FOLDERID_ProgramFiles)?),
            program_files_x86: Some(known(&FOLDERID_ProgramFilesX86)?),
            program_data: Some(known(&FOLDERID_ProgramData)?),
            profile: known(&FOLDERID_Profile)?,
            profiles: Some(known(&FOLDERID_UserProfiles)?),
            exe_folder: exe_folder()?,
            scratch_root: canonical(scratch_root)?,
        })
    }

    pub(super) fn check(&self, path: &Path) -> Result<(), SourceError> {
        if unc_server_root(path) {
            return Err(SourceError::UncServerRoot);
        }
        if path.has_root() && path.parent().is_none() {
            return Err(if self.is_system_root(path) {
                SourceError::SystemDriveRoot
            } else {
                SourceError::DriveRoot
            });
        }
        let system = [
            (self.windows.as_deref(), SourceError::WindowsFolder),
            (self.program_files.as_deref(), SourceError::ProgramFiles),
            (
                self.program_files_x86.as_deref(),
                SourceError::ProgramFilesX86,
            ),
            (self.program_data.as_deref(), SourceError::ProgramData),
        ];
        for (folder, error) in system {
            if folder.is_some_and(|folder| overlaps(path, folder)) {
                return Err(error);
            }
        }
        self.check_profiles(path)?;
        if overlaps(path, &self.exe_folder) {
            return Err(SourceError::ExeFolder);
        }
        if overlaps(path, &self.scratch_root) {
            return Err(SourceError::ScratchRoot);
        }
        Ok(())
    }

    /// Children of a profile stay allowed; the profile itself and anything holding it are not.
    fn check_profiles(&self, path: &Path) -> Result<(), SourceError> {
        if inside(&self.profile, path) {
            return Err(SourceError::UserProfileRoot);
        }
        if self.profiles.as_deref().is_some_and(|profiles| {
            inside(profiles, path)
                || path
                    .parent()
                    .is_some_and(|parent| same_path(parent, profiles))
        }) {
            return Err(SourceError::UserProfileRoot);
        }
        Ok(())
    }

    fn is_system_root(&self, path: &Path) -> bool {
        self.windows
            .as_deref()
            .and_then(|folder| folder.ancestors().last())
            .is_some_and(|root| same_path(path, root))
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        normalized(left) == normalized(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

/// True when `path` is `folder` or lies anywhere below it.
fn inside(path: &Path, folder: &Path) -> bool {
    #[cfg(windows)]
    {
        let (path, folder) = (normalized(path), normalized(folder));
        let (path, folder) = (path.trim_end_matches('\\'), folder.trim_end_matches('\\'));
        path == folder
            || path
                .strip_prefix(folder)
                .is_some_and(|rest| rest.starts_with('\\'))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(folder)
    }
}

fn overlaps(path: &Path, folder: &Path) -> bool {
    inside(path, folder) || inside(folder, path)
}

/// Windows names cannot contain `/`, so it compares as the separator it stands for.
#[cfg(windows)]
fn normalized(path: &Path) -> String {
    path.as_os_str()
        .to_string_lossy()
        .replace('/', "\\")
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}

fn exe_folder() -> Result<PathBuf, SourceError> {
    let exe = std::env::current_exe().map_err(|_| SourceError::FolderLookup)?;
    let exe = canonical(&exe)?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or(SourceError::FolderLookup)
}

fn canonical(path: &Path) -> Result<PathBuf, SourceError> {
    std::fs::canonicalize(path).map_err(|_| SourceError::FolderLookup)
}

fn unc_server_root(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    let text = text
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| text.strip_prefix(r"\\"));
    text.is_some_and(|tail| !tail.trim_matches(['\\', '/']).contains(['\\', '/']))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "known-folder lookup requires Windows FFI and owns its returned allocation"
)]
fn known(folder: &windows::core::GUID) -> Result<PathBuf, SourceError> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    // SAFETY: folder points to a live GUID; the API returns an owned UTF-16 allocation.
    let raw = unsafe { SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None) }
        .map_err(|_| SourceError::FolderLookup)?;
    // SAFETY: a successful lookup returns a valid NUL-terminated allocation.
    let path = unsafe { raw.to_string() }.map(PathBuf::from);
    // SAFETY: raw is the allocation returned by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(Some(raw.0.cast())) };
    let path = path.map_err(|_| SourceError::FolderLookup)?;
    std::fs::canonicalize(path).map_err(|_| SourceError::FolderLookup)
}
