//! The shell side of a recycle: one `IFileOperation` whose sink vetoes every delete but a
//! Recycle Bin move of the verified item.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::E_ABORT;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IBindCtx,
};
use windows::Win32::UI::Shell::{
    FILEOPERATION_FLAGS, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
    FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IFileOperationProgressSink,
    IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows_core::{HRESULT, IUnknown, PCWSTR, Ref, implement};

use super::act::Identity;
use super::veto::{self, Veto};
use super::{PathRole, RecycleError, act_platform, platform};

const _: () = assert!(veto::RECYCLE_FLAG == TSF_DELETE_RECYCLE_IF_POSSIBLE.0.cast_unsigned());

/// No dialog may block an unattended run; the sink's veto replaces the confirmations.
pub(super) const OPERATION_FLAGS: FILEOPERATION_FLAGS = FILEOPERATION_FLAGS(
    FOFX_RECYCLEONDELETE.0
        | FOF_ALLOWUNDO.0
        | FOF_NOCONFIRMATION.0
        | FOF_NOERRORUI.0
        | FOF_SILENT.0,
);

/// Moves the item at `path` to the Recycle Bin when it still has `expected` identity.
pub(super) fn recycle(path: &Path, expected: Identity) -> Result<(), RecycleError> {
    let _com = com_init();
    perform(path, expected)
}

fn perform(path: &Path, expected: Identity) -> Result<(), RecycleError> {
    let wide = act_platform::wide_nul(path.as_os_str()).map_err(|_| RecycleError::Io)?;
    // SAFETY: `wide` is NUL-terminated and outlives the call; COM is initialised on this thread.
    let item: IShellItem =
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>) }
            .map_err(|_| RecycleError::Shell)?;
    require_same_path(&item, path)?;
    // SAFETY: COM is initialised on this thread; the class id is the shell's file operation.
    let operation: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None::<&IUnknown>, CLSCTX_ALL) }
            .map_err(|_| RecycleError::Shell)?;
    // SAFETY: `operation` is a live interface; the flags ask for a silent Recycle Bin move.
    unsafe { operation.SetOperationFlags(OPERATION_FLAGS) }.map_err(|_| RecycleError::Shell)?;
    let sink = windows_core::ComObject::new(Sink::new(expected));
    let callback: IFileOperationProgressSink = sink.to_interface();
    // SAFETY: both interfaces are live; the cookie is released below.
    let cookie = unsafe { operation.Advise(&callback) }.map_err(|_| RecycleError::Shell)?;
    let outcome = run(&operation, &item);
    // SAFETY: `cookie` came from Advise on this operation.
    let _ = unsafe { operation.Unadvise(cookie) };
    sink.verdict(outcome)
}

fn require_same_path(item: &IShellItem, path: &Path) -> Result<(), RecycleError> {
    if !platform::equal(&file_system_path(item)?, path.as_os_str()) {
        return Err(RecycleError::Shell);
    }
    Ok(())
}

fn file_system_path(item: &IShellItem) -> Result<OsString, RecycleError> {
    // SAFETY: `item` is a live shell item.
    let name =
        unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|_| RecycleError::Shell)?;
    // SAFETY: the shell returned a NUL-terminated string that stays valid until freed below.
    let text = OsString::from_wide(unsafe { name.as_wide() });
    // SAFETY: the string was allocated by the shell with the COM allocator and is freed once.
    unsafe { CoTaskMemFree(Some(name.0.cast_const().cast())) };
    Ok(text)
}

/// Identity of the item the shell is about to delete, opened without following links;
/// `None` for a link or anything that cannot be read, which the veto refuses.
fn identity_of(item: Option<&IShellItem>) -> Option<Identity> {
    let text = file_system_path(item?).ok()?;
    let units: Vec<u16> = text.encode_wide().collect();
    let drive = matches!(units.get(1..3), Some([0x3A, 0x5C]));
    let path = if drive {
        let mut verbatim = OsString::from(r"\\?\");
        verbatim.push(&text);
        PathBuf::from(verbatim)
    } else {
        PathBuf::from(text)
    };
    let handle = act_platform::open_attributes(&path).ok()?;
    let role = PathRole::ScratchRoot;
    if act_platform::is_reparse(&handle, role).unwrap_or(true) {
        return None;
    }
    act_platform::identity(&handle, role).ok()
}

#[derive(Default)]
struct Outcome {
    performed: bool,
    aborted: bool,
}

fn run(operation: &IFileOperation, item: &IShellItem) -> Outcome {
    // SAFETY: both interfaces are live; no per-item sink is passed.
    let queued = unsafe { operation.DeleteItem(item, None::<&IFileOperationProgressSink>) };
    if queued.is_err() {
        return Outcome::default();
    }
    // SAFETY: `operation` is live and has exactly one queued delete.
    let performed = unsafe { operation.PerformOperations() }.is_ok();
    // SAFETY: `operation` is live and has finished performing.
    let aborted =
        unsafe { operation.GetAnyOperationsAborted() }.map_or(true, |flag| flag.as_bool());
    Outcome { performed, aborted }
}

/// Refuses a permanent delete, any item but the expected one and any second delete, and records
/// how the delete ended.
#[implement(IFileOperationProgressSink)]
struct Sink {
    expected: Identity,
    allowed: AtomicBool,
    refused: AtomicBool,
    other: AtomicBool,
    recycled: AtomicBool,
    failed: AtomicBool,
}

impl Sink {
    fn new(expected: Identity) -> Self {
        Self {
            expected,
            allowed: AtomicBool::new(false),
            refused: AtomicBool::new(false),
            other: AtomicBool::new(false),
            recycled: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        }
    }

    fn verdict(&self, outcome: Outcome) -> Result<(), RecycleError> {
        if self.refused.load(Ordering::SeqCst) {
            return Err(RecycleError::PermanentDelete);
        }
        if self.other.load(Ordering::SeqCst) {
            return Err(RecycleError::OtherItem);
        }
        if outcome.aborted {
            return Err(RecycleError::Aborted);
        }
        let failed = self.failed.load(Ordering::SeqCst) || !self.recycled.load(Ordering::SeqCst);
        if !outcome.performed || failed {
            return Err(RecycleError::Shell);
        }
        Ok(())
    }
}

type Item<'a> = Ref<'a, IShellItem>;

impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn PreDeleteItem(&self, flags: u32, item: Item<'_>) -> windows_core::Result<()> {
        let veto = match veto::pre_delete(flags, identity_of(item.as_ref()), self.expected) {
            Veto::Allow if self.allowed.swap(true, Ordering::SeqCst) => Veto::OtherItem,
            other => other,
        };
        let flag = match veto {
            Veto::Allow => return Ok(()),
            Veto::Permanent => &self.refused,
            Veto::OtherItem => &self.other,
        };
        flag.store(true, Ordering::SeqCst);
        Err(E_ABORT.into())
    }

    fn PostDeleteItem(
        &self,
        _flags: u32,
        _item: Item<'_>,
        result: HRESULT,
        recycled: Item<'_>,
    ) -> windows_core::Result<()> {
        let target = if result.is_ok() && !recycled.is_null() {
            &self.recycled
        } else {
            &self.failed
        };
        target.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn StartOperations(&self) -> windows_core::Result<()> {
        Ok(())
    }

    fn FinishOperations(&self, _result: HRESULT) -> windows_core::Result<()> {
        Ok(())
    }

    fn PreRenameItem(&self, _: u32, _: Item<'_>, _: &PCWSTR) -> windows_core::Result<()> {
        Ok(())
    }

    fn PostRenameItem(
        &self,
        _: u32,
        _: Item<'_>,
        _: &PCWSTR,
        _: HRESULT,
        _: Item<'_>,
    ) -> windows_core::Result<()> {
        Ok(())
    }

    fn PreMoveItem(
        &self,
        _: u32,
        _: Item<'_>,
        _: Item<'_>,
        _: &PCWSTR,
    ) -> windows_core::Result<()> {
        Ok(())
    }

    fn PostMoveItem(
        &self,
        _: u32,
        _: Item<'_>,
        _: Item<'_>,
        _: &PCWSTR,
        _: HRESULT,
        _: Item<'_>,
    ) -> windows_core::Result<()> {
        Ok(())
    }

    fn PreCopyItem(
        &self,
        _: u32,
        _: Item<'_>,
        _: Item<'_>,
        _: &PCWSTR,
    ) -> windows_core::Result<()> {
        Ok(())
    }

    fn PostCopyItem(
        &self,
        _: u32,
        _: Item<'_>,
        _: Item<'_>,
        _: &PCWSTR,
        _: HRESULT,
        _: Item<'_>,
    ) -> windows_core::Result<()> {
        Ok(())
    }

    fn PreNewItem(&self, _: u32, _: Item<'_>, _: &PCWSTR) -> windows_core::Result<()> {
        Ok(())
    }

    fn PostNewItem(
        &self,
        _: u32,
        _: Item<'_>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Item<'_>,
    ) -> windows_core::Result<()> {
        Ok(())
    }

    fn UpdateProgress(&self, _total: u32, _done: u32) -> windows_core::Result<()> {
        Ok(())
    }

    fn ResetTimer(&self) -> windows_core::Result<()> {
        Ok(())
    }

    fn PauseTimer(&self) -> windows_core::Result<()> {
        Ok(())
    }

    fn ResumeTimer(&self) -> windows_core::Result<()> {
        Ok(())
    }
}

/// Initialises COM for one recycle and balances it on drop.
struct Com(bool);

fn com_init() -> Com {
    // SAFETY: plain per-thread COM initialisation, balanced in Drop when it succeeded.
    let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    Com(result.is_ok())
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful CoInitializeEx in `com_init` on this thread.
            unsafe { CoUninitialize() };
        }
    }
}
