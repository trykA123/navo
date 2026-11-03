mod kind;
mod medium;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, channel, sync_channel};

use windows::Win32::Foundation::{DV_E_FORMATETC, DV_E_TYMED, E_ABORT, S_OK};
use windows::Win32::System::Com::{IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    DROPEFFECT_COPY, OleGetClipboard, OleInitialize, OleUninitialize,
};
use windows::Win32::UI::Shell::IDataObjectAsyncCapability;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};
use windows::core::AgileReference;

use super::descriptor::{Names, parse};
use super::reader::STALL;
use super::serve::{Backend, Opened, Request, serve};
use super::{SourceKind, VirtualClipboard, VirtualEntry, VirtualError};
use crate::clipboard::platform::{decode_ansi, format_available};
use crate::clipboard::watch::register;

pub(super) enum Origin {
    Clipboard,
    Object(AgileReference<IDataObject>),
}

/// Registered clipboard formats; `None` when registration failed.
pub(super) struct Formats {
    wide: Option<u16>,
    ansi: Option<u16>,
    contents: Option<u16>,
    mail: Option<u16>,
    id_list: Option<u16>,
}

impl Formats {
    fn new() -> Self {
        let format = |name| register(name).ok().and_then(|id| u16::try_from(id).ok());
        Self {
            wide: format("FileGroupDescriptorW"),
            ansi: format("FileGroupDescriptor"),
            contents: format("FileContents"),
            mail: format("RenPrivateMessages"),
            id_list: format("Shell IDList Array"),
        }
    }
}

pub(super) fn offered() -> bool {
    let formats = Formats::new();
    [formats.wide, formats.ansi]
        .into_iter()
        .flatten()
        .any(|format| format_available(u32::from(format)))
}

pub(super) fn read_clipboard() -> Result<Option<VirtualClipboard>, VirtualError> {
    if !offered() {
        return Ok(None);
    }
    start(Origin::Clipboard)
}

/// A data object made usable from the reader thread.
pub struct VirtualSource(AgileReference<IDataObject>);

impl VirtualSource {
    /// Call on the thread that received `object`; it does not block.
    pub fn new(object: &IDataObject) -> Result<Self, VirtualError> {
        AgileReference::new(object).map(Self).map_err(com_error)
    }
}

pub(super) fn read_object(source: VirtualSource) -> Result<Option<VirtualClipboard>, VirtualError> {
    start(Origin::Object(source.0))
}

/// The entries, where they come from, and the .zip file when it is known.
type Found = (Vec<VirtualEntry>, SourceKind, Option<PathBuf>);
type Listed = Result<Option<Found>, VirtualError>;

/// Starts the reader thread and waits for the file list. A source that never answers ends
/// the wait after the stall time; its thread exits once the COM call returns.
fn start(origin: Origin) -> Result<Option<VirtualClipboard>, VirtualError> {
    let (listed, list) = sync_channel::<Listed>(1);
    let (requests, received) = channel();
    std::thread::Builder::new()
        .name("navo-virtual-files".into())
        .spawn(move || run(origin, &listed, &received))
        .map_err(|error| VirtualError::Source(error.to_string()))?;
    let (entries, kind, archive) = match list.recv_timeout(STALL) {
        Ok(listed) => match listed? {
            Some(found) => found,
            None => return Ok(None),
        },
        Err(RecvTimeoutError::Timeout) => return Err(VirtualError::Stalled),
        Err(RecvTimeoutError::Disconnected) => {
            return Err(VirtualError::Source(
                "The virtual file reader stopped".into(),
            ));
        }
    };
    Ok(Some(VirtualClipboard::new(
        (entries, kind, archive),
        requests,
    )))
}

fn run(origin: Origin, listed: &SyncSender<Listed>, requests: &Receiver<Request>) {
    // SAFETY: first COM call on this new thread; balanced by OleUninitialize below.
    if let Err(error) = unsafe { OleInitialize(None) } {
        let _ = listed.send(Err(com_error(error)));
        return;
    }
    serve_object(origin, listed, requests);
    // SAFETY: OleInitialize succeeded on this thread and every COM object is released.
    unsafe { OleUninitialize() };
}

fn serve_object(origin: Origin, listed: &SyncSender<Listed>, requests: &Receiver<Request>) {
    let object = match &origin {
        // SAFETY: OLE is initialised on this thread.
        Origin::Clipboard => unsafe { OleGetClipboard() },
        Origin::Object(agile) => agile.resolve(),
    };
    let formats = Formats::new();
    let found = object
        .map_err(com_error)
        .and_then(|object| list(&object, &formats).map(|entries| (object, entries)));
    let (object, entries) = match found {
        Ok((object, Some(entries))) => (object, entries),
        Ok((_, None)) => {
            let _ = listed.send(Ok(None));
            return;
        }
        Err(error) => {
            let _ = listed.send(Err(error));
            return;
        }
    };
    let (kind, archive) = kind::source_kind(&object, &formats, &origin);
    if listed
        .send(Ok(Some((entries.clone(), kind, archive))))
        .is_err()
    {
        return;
    }
    let mut backend = OleBackend::new(object, formats.contents, &entries);
    serve(&mut backend, requests);
}

/// The descriptor's entries; the wide one wins over the ANSI one.
fn list(
    object: &IDataObject,
    formats: &Formats,
) -> Result<Option<Vec<VirtualEntry>>, VirtualError> {
    let decode = |bytes: &[u8]| {
        decode_ansi(bytes)
            .ok()
            .and_then(|path| path.into_os_string().into_string().ok())
    };
    let choices = [
        (formats.wide, Names::Wide),
        (formats.ansi, Names::Ansi(&decode)),
    ];
    for (format, names) in choices {
        let Some(format) = format else { continue };
        let medium = match medium::Medium::get(object, format, -1, TYMED_HGLOBAL) {
            Ok(medium) => medium,
            Err(error) if [DV_E_FORMATETC, DV_E_TYMED].contains(&error.code()) => continue,
            Err(error) => {
                let reason = error.message();
                return Err(VirtualError::Source(format!(
                    "Reading the file list: {reason}"
                )));
            }
        };
        let bytes = medium
            .global_bytes(usize::MAX)
            .map_err(VirtualError::Source)?;
        let entries = parse(&bytes, &names)?;
        return Ok((!entries.is_empty()).then_some(entries));
    }
    Ok(None)
}

struct OleBackend {
    object: IDataObject,
    contents: Option<u16>,
    sizes: Vec<Option<u64>>,
    operation: Option<IDataObjectAsyncCapability>,
}

impl OleBackend {
    fn new(object: IDataObject, contents: Option<u16>, entries: &[VirtualEntry]) -> Self {
        let operation = start_operation(&object);
        Self {
            object,
            contents,
            sizes: entries.iter().map(|entry| entry.size).collect(),
            operation,
        }
    }

    fn target(&self, index: u32) -> Result<(u16, i32), String> {
        let format = self
            .contents
            .ok_or("The FileContents format is not available")?;
        let index = i32::try_from(index).map_err(|_| "The item index is too large")?;
        Ok((format, index))
    }
}

impl Backend for OleBackend {
    fn open(&mut self, index: u32) -> Result<Opened, String> {
        let size = usize::try_from(index)
            .ok()
            .and_then(|at| self.sizes.get(at).copied().flatten());
        medium::open(&self.object, self.target(index)?, size)
    }

    fn write_storage(
        &mut self,
        index: u32,
        path: &Path,
        created: &mut dyn FnMut(),
    ) -> Result<(), String> {
        medium::write_storage(&self.object, self.target(index)?, path, created)
    }

    fn idle(&mut self) {
        pump_messages();
    }

    fn finish(&mut self, complete: bool) {
        let Some(operation) = self.operation.take() else {
            return;
        };
        let result = if complete { S_OK } else { E_ABORT };
        // SAFETY: StartOperation succeeded on this object; EndOperation runs once.
        let _ = unsafe { operation.EndOperation(result, None, DROPEFFECT_COPY.0) };
    }
}

/// Starts an async extraction (zip folders, some phones) when the source asks for one.
fn start_operation(object: &IDataObject) -> Option<IDataObjectAsyncCapability> {
    let operation: IDataObjectAsyncCapability = windows::core::Interface::cast(object).ok()?;
    // SAFETY: operation is a live interface on this thread's apartment.
    let is_async = unsafe { operation.GetAsyncMode() }.ok()?.as_bool();
    // SAFETY: as above; no bind context is passed.
    (is_async && unsafe { operation.StartOperation(None) }.is_ok()).then_some(operation)
}

/// Dispatches this thread's messages, as an STA thread must while it waits.
fn pump_messages() {
    let mut msg = MSG::default();
    // SAFETY: dispatches messages queued for this thread only.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn com_error(error: windows::core::Error) -> VirtualError {
    VirtualError::Source(error.message())
}
