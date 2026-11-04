use std::fmt;
use std::path::PathBuf;

/// The picker failed. Holds the HRESULT only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickerError {
    Unsupported,
    Failed(i32),
}

impl fmt::Display for PickerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("picker:unsupported"),
            Self::Failed(code) => write!(f, "picker:hresult-{code:#010x}"),
        }
    }
}

impl std::error::Error for PickerError {}

/// Shows the folder picker. `Ok(None)` means the user cancelled.
#[cfg(not(windows))]
pub fn pick_folder() -> Result<Option<PathBuf>, PickerError> {
    Err(PickerError::Unsupported)
}

#[cfg(windows)]
pub fn pick_folder() -> Result<Option<PathBuf>, PickerError> {
    windows_picker::pick()
}

#[cfg(windows)]
mod windows_picker {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
    };

    use super::PickerError;

    fn failed(error: &windows::core::Error) -> PickerError {
        PickerError::Failed(error.code().0)
    }

    #[allow(
        unsafe_code,
        reason = "COM apartment setup and teardown on this thread"
    )]
    pub(super) fn pick() -> Result<Option<PathBuf>, PickerError> {
        // SAFETY: balanced by CoUninitialize below once the dialog is dropped.
        let init =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        if init.is_err() {
            return Err(PickerError::Failed(init.0));
        }
        let result = show();
        // SAFETY: matches the successful CoInitializeEx above; `show` has dropped its COM objects.
        unsafe { CoUninitialize() };
        result
    }

    #[allow(
        unsafe_code,
        reason = "IFileOpenDialog calls and freeing the returned string"
    )]
    fn show() -> Result<Option<PathBuf>, PickerError> {
        // SAFETY: plain COM calls on an apartment initialised by `pick`.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| failed(&e))?;
            let options = dialog.GetOptions().map_err(|e| failed(&e))?;
            dialog
                .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
                .map_err(|e| failed(&e))?;
            if let Err(error) = dialog.Show(None) {
                return if error.code() == ERROR_CANCELLED.to_hresult() {
                    Ok(None)
                } else {
                    Err(failed(&error))
                };
            }
            let item = dialog.GetResult().map_err(|e| failed(&e))?;
            let name = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|e| failed(&e))?;
            let path = PathBuf::from(OsString::from_wide(name.as_wide()));
            CoTaskMemFree(Some(name.0.cast_const().cast()));
            Ok(Some(path))
        }
    }
}
