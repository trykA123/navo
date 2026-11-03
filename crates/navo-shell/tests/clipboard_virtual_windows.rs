#![cfg(windows)]

use std::io::Read;
use std::mem::ManuallyDrop;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navo_shell::{
    Cancel, SourceKind, VirtualClipboard, VirtualContent, VirtualSource, has_virtual_files,
    read_virtual_files, read_virtual_files_from,
};
use windows::Win32::Foundation::{
    DV_E_FORMATETC, E_NOTIMPL, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, S_OK,
};
use windows::Win32::System::Com::StructuredStorage::{IStorage, StgCreateDocfile, StgOpenStorage};
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IAdviseSink, IBindCtx, IDataObject, IDataObject_Impl,
    IEnumFORMATETC, IEnumSTATDATA, STGM_CREATE, STGM_DELETEONRELEASE, STGM_READ, STGM_READWRITE,
    STGM_SHARE_EXCLUSIVE, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL, TYMED_ISTORAGE, TYMED_ISTREAM,
};
use windows::Win32::System::Ole::{OleInitialize, OleSetClipboard, OleUninitialize};
use windows::Win32::UI::Shell::{
    IDataObjectAsyncCapability, IDataObjectAsyncCapability_Impl, SHCreateMemStream,
    SHCreateStdEnumFmtEtc,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};
use windows::core::{BOOL, HRESULT, PCWSTR, Ref, implement, w};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

static CLIPBOARD: Mutex<()> = Mutex::new(());

#[derive(Clone)]
enum Content {
    Stream(Vec<u8>),
    Storage(&'static [u8]),
}

#[implement(IDataObject, IDataObjectAsyncCapability)]
struct Offer {
    descriptor_format: u16,
    descriptor: Vec<u8>,
    contents_format: u16,
    contents: Vec<Content>,
    mail_format: Option<u16>,
    async_mode: bool,
    started: Arc<AtomicU32>,
    ended: Arc<AtomicU32>,
    end_result: Arc<Mutex<Option<HRESULT>>>,
}

#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn format(name: &str) -> u16 {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: wide is a live NUL-terminated UTF-16 string.
    u16::try_from(unsafe { RegisterClipboardFormatW(wide.as_ptr()) }).unwrap()
}

#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn tymed(value: windows::Win32::System::Com::TYMED) -> u32 {
    u32::try_from(value.0).unwrap()
}

fn global(bytes: &[u8]) -> STGMEDIUM {
    // SAFETY: a nonzero movable allocation that the medium's receiver frees.
    let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
    assert!(!block.is_null(), "the test needs clipboard memory");
    // SAFETY: block is live; the copy stays inside its size; it is unlocked once.
    unsafe {
        let data = GlobalLock(block).cast::<u8>();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        GlobalUnlock(block);
    }
    STGMEDIUM {
        tymed: tymed(TYMED_HGLOBAL),
        u: STGMEDIUM_0 {
            hGlobal: HGLOBAL(block),
        },
        pUnkForRelease: ManuallyDrop::new(None),
    }
}

#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn mail_storage(subject: &[u8]) -> windows::core::Result<IStorage> {
    let mode = STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE | STGM_DELETEONRELEASE;
    // SAFETY: a temporary docfile with no name; every pointer argument is valid.
    unsafe {
        let storage = StgCreateDocfile(PCWSTR::null(), mode, None)?;
        let stream = storage.CreateStream(
            w!("subject"),
            STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
            0,
            0,
        )?;
        let length = u32::try_from(subject.len()).unwrap();
        stream.Write(subject.as_ptr().cast(), length, None).ok()?;
        storage.Commit(0)?;
        Ok(storage)
    }
}

impl Offer_Impl {
    fn item(&self, index: i32) -> windows::core::Result<STGMEDIUM> {
        let content = usize::try_from(index)
            .ok()
            .and_then(|at| self.contents.get(at))
            .ok_or(DV_E_FORMATETC)?;
        Ok(match content {
            Content::Stream(bytes) => STGMEDIUM {
                tymed: tymed(TYMED_ISTREAM),
                u: STGMEDIUM_0 {
                    // SAFETY: the bytes are copied into the new stream.
                    pstm: ManuallyDrop::new(unsafe { SHCreateMemStream(Some(bytes)) }),
                },
                pUnkForRelease: ManuallyDrop::new(None),
            },
            Content::Storage(subject) => STGMEDIUM {
                tymed: tymed(TYMED_ISTORAGE),
                u: STGMEDIUM_0 {
                    pstg: ManuallyDrop::new(Some(mail_storage(subject)?)),
                },
                pUnkForRelease: ManuallyDrop::new(None),
            },
        })
    }
}

impl IDataObject_Impl for Offer_Impl {
    fn GetData(&self, format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        // SAFETY: COM passes a valid FORMATETC for the duration of the call.
        let format = unsafe { &*format };
        if format.cfFormat == self.descriptor_format && format.tymed & tymed(TYMED_HGLOBAL) != 0 {
            return Ok(global(&self.descriptor));
        }
        if Some(format.cfFormat) == self.mail_format {
            return Ok(global(&[0; 4]));
        }
        if format.cfFormat == self.contents_format {
            return self.item(format.lindex);
        }
        Err(DV_E_FORMATETC.into())
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        // SAFETY: COM passes a valid FORMATETC for the duration of the call.
        let format = unsafe { &*format }.cfFormat;
        let offered = [self.descriptor_format, self.contents_format];
        if offered.contains(&format) || Some(format) == self.mail_format {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(
        &self,
        _: *const FORMATETC,
        _: *const STGMEDIUM,
        _: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, _: u32) -> windows::core::Result<IEnumFORMATETC> {
        let entry = |format, tymed_flags| FORMATETC {
            cfFormat: format,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: tymed_flags,
        };
        let mut formats = vec![
            entry(self.descriptor_format, tymed(TYMED_HGLOBAL)),
            entry(
                self.contents_format,
                tymed(TYMED_ISTREAM) | tymed(TYMED_ISTORAGE),
            ),
        ];
        formats.extend(
            self.mail_format
                .map(|format| entry(format, tymed(TYMED_HGLOBAL))),
        );
        // SAFETY: the shell copies the array.
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }

    fn DAdvise(
        &self,
        _: *const FORMATETC,
        _: u32,
        _: Ref<IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

impl IDataObjectAsyncCapability_Impl for Offer_Impl {
    fn SetAsyncMode(&self, _: BOOL) -> windows::core::Result<()> {
        Ok(())
    }

    fn GetAsyncMode(&self) -> windows::core::Result<BOOL> {
        Ok(self.async_mode.into())
    }

    fn StartOperation(&self, _: Ref<IBindCtx>) -> windows::core::Result<()> {
        self.started.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn InOperation(&self) -> windows::core::Result<BOOL> {
        Ok((self.started.load(Ordering::SeqCst) > self.ended.load(Ordering::SeqCst)).into())
    }

    #[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
    fn EndOperation(&self, result: HRESULT, _: Ref<IBindCtx>, _: u32) -> windows::core::Result<()> {
        self.ended.fetch_add(1, Ordering::SeqCst);
        *self.end_result.lock().unwrap() = Some(result);
        Ok(())
    }
}

struct Fd {
    flags: u32,
    attributes: u32,
    size: u64,
    name: &'static str,
}

#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn file(name: &'static str, size: usize) -> Fd {
    Fd {
        flags: 0x4 | 0x40,
        attributes: 0x80,
        size: u64::try_from(size).unwrap(),
        name,
    }
}

fn folder(name: &'static str) -> Fd {
    Fd {
        flags: 0x4,
        attributes: 0x10,
        size: 0,
        name,
    }
}

#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn descriptor(entries: &[Fd], wide: bool) -> Vec<u8> {
    let mut bytes = u32::try_from(entries.len()).unwrap().to_le_bytes().to_vec();
    for fd in entries {
        let mut raw = vec![0u8; 72];
        raw[0..4].copy_from_slice(&fd.flags.to_le_bytes());
        raw[36..40].copy_from_slice(&fd.attributes.to_le_bytes());
        raw[64..68].copy_from_slice(&u32::try_from(fd.size >> 32).unwrap().to_le_bytes());
        raw[68..72].copy_from_slice(&u32::try_from(fd.size & 0xffff_ffff).unwrap().to_le_bytes());
        if wide {
            let mut units: Vec<u16> = fd.name.encode_utf16().collect();
            units.resize(260, 0);
            raw.extend(units.into_iter().flat_map(u16::to_le_bytes));
        } else {
            let mut name = fd.name.as_bytes().to_vec();
            name.resize(260, 0);
            raw.extend(name);
        }
        bytes.extend(raw);
    }
    bytes
}

struct Counters {
    started: Arc<AtomicU32>,
    ended: Arc<AtomicU32>,
    end_result: Arc<Mutex<Option<HRESULT>>>,
}

fn offer(entries: &[Fd], contents: Vec<Content>, wide: bool) -> (Offer, Counters) {
    let counters = Counters {
        started: Arc::default(),
        ended: Arc::default(),
        end_result: Arc::default(),
    };
    let offer = Offer {
        descriptor_format: format(if wide {
            "FileGroupDescriptorW"
        } else {
            "FileGroupDescriptor"
        }),
        descriptor: descriptor(entries, wide),
        contents_format: format("FileContents"),
        contents,
        mail_format: None,
        async_mode: false,
        started: Arc::clone(&counters.started),
        ended: Arc::clone(&counters.ended),
        end_result: Arc::clone(&counters.end_result),
    };
    (offer, counters)
}

struct Ole;

impl Ole {
    #[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
    fn init() -> Self {
        // SAFETY: first COM call on this test thread; balanced in Drop.
        unsafe { OleInitialize(None) }.unwrap();
        Self
    }
}

impl Drop for Ole {
    fn drop(&mut self) {
        // SAFETY: drop the clipboard object first, then end OLE on this thread.
        unsafe {
            let _ = OleSetClipboard(None::<&IDataObject>);
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                EmptyClipboard();
                CloseClipboard();
            }
            OleUninitialize();
        }
    }
}

/// Runs `work` on another thread while this STA thread serves calls into its objects.
#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn pumping<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let worker = std::thread::spawn(work);
    let deadline = Instant::now() + Duration::from_secs(60);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline, "the reader must finish in time");
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap()
}

fn pump() {
    let mut msg = MSG::default();
    // SAFETY: dispatches messages queued for this thread only.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn never() -> Cancel {
    Arc::new(|| false)
}

/// Writes every valid entry below `root`, as the job side will.
#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn paste_into(clipboard: &VirtualClipboard, root: &Path) {
    for entry in clipboard.entries() {
        let Ok(path) = &entry.path else { continue };
        let target = root.join(path);
        if entry.is_dir {
            std::fs::create_dir_all(&target).unwrap();
            continue;
        }
        match clipboard.open(entry.index, never()).unwrap() {
            VirtualContent::Stream(mut reader) => {
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).unwrap();
                std::fs::write(&target, bytes).unwrap();
            }
            VirtualContent::Storage => {
                clipboard
                    .write_storage(entry.index, (&target, &mut || Ok(())), &never())
                    .unwrap();
            }
        }
    }
}

#[expect(clippy::unwrap_used, reason = "a failed test setup must stop the test")]
fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|at| u8::try_from(at % 251).unwrap()).collect()
}

#[test]
fn wide_virtual_files_paste_from_the_clipboard_with_folders_and_refused_names() {
    let _serial = CLIPBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ole = Ole::init();
    let big = pattern(300 * 1024);
    let entries = [
        folder("Docs"),
        file("Docs\\a.txt", 5),
        file("big.bin", big.len()),
        file("..\\escape.txt", 3),
    ];
    let contents = vec![
        Content::Stream(Vec::new()),
        Content::Stream(b"alpha".to_vec()),
        Content::Stream(big.clone()),
        Content::Stream(b"bad".to_vec()),
    ];
    let (offer, _) = offer(&entries, contents, true);
    let object: IDataObject = offer.into();
    // SAFETY: OLE is initialised on this thread and object is a live data object.
    unsafe { OleSetClipboard(&object) }.unwrap();
    assert!(has_virtual_files());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("target");
    std::fs::create_dir(&root).unwrap();
    let pasted = root.clone();
    let kind = pumping(move || {
        let clipboard = read_virtual_files()
            .unwrap()
            .expect("virtual files are offered");
        paste_into(&clipboard, &pasted);
        clipboard.source_kind()
    });
    assert_eq!(kind, SourceKind::Unknown);
    assert_eq!(
        std::fs::read(root.join("Docs").join("a.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(std::fs::read(root.join("big.bin")).unwrap(), big);
    assert!(
        !dir.path().join("escape.txt").exists(),
        "a traversal name writes nothing"
    );
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["Docs", "big.bin"]);
}

#[test]
fn an_ansi_descriptor_is_read_when_the_wide_one_is_absent() {
    let _serial = CLIPBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ole = Ole::init();
    let (offer, _) = offer(
        &[file("plain.txt", 4)],
        vec![Content::Stream(b"ansi".to_vec())],
        false,
    );
    let object: IDataObject = offer.into();
    // SAFETY: OLE is initialised on this thread and object is a live data object.
    unsafe { OleSetClipboard(&object) }.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pasted = dir.path().to_path_buf();
    pumping(move || {
        let clipboard = read_virtual_files()
            .unwrap()
            .expect("an ANSI descriptor is offered");
        assert_eq!(clipboard.entries()[0].size, Some(4));
        paste_into(&clipboard, &pasted);
    });
    assert_eq!(
        std::fs::read(dir.path().join("plain.txt")).unwrap(),
        b"ansi"
    );
}

#[test]
fn an_outlook_storage_item_is_saved_as_a_compound_file() {
    let _serial = CLIPBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ole = Ole::init();
    let (mut offer, _) = offer(
        &[file("Meeting.msg", 0)],
        vec![Content::Storage(b"Quarterly review")],
        true,
    );
    offer.mail_format = Some(format("RenPrivateMessages"));
    let object: IDataObject = offer.into();
    // SAFETY: OLE is initialised on this thread and object is a live data object.
    unsafe { OleSetClipboard(&object) }.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pasted = dir.path().to_path_buf();
    let kind = pumping(move || {
        let clipboard = read_virtual_files()
            .unwrap()
            .expect("a mail item is offered");
        paste_into(&clipboard, &pasted);
        clipboard.source_kind()
    });
    assert_eq!(kind, SourceKind::Mail);
    let path: Vec<u16> = dir
        .path()
        .join("Meeting.msg")
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    let mode = STGM_READ | STGM_SHARE_EXCLUSIVE;
    // SAFETY: path is NUL-terminated and outlives the calls; buf bounds the read.
    let subject = unsafe {
        let storage = StgOpenStorage(PCWSTR(path.as_ptr()), None, mode, None, 0).unwrap();
        let stream = storage.OpenStream(w!("subject"), None, mode, 0).unwrap();
        let mut buf = [0u8; 64];
        let mut read = 0u32;
        stream
            .Read(buf.as_mut_ptr().cast(), 64, Some(&mut read))
            .ok()
            .unwrap();
        buf[..usize::try_from(read).unwrap()].to_vec()
    };
    assert_eq!(subject, b"Quarterly review");
}

#[test]
fn an_async_drop_source_is_started_and_ended_once() {
    let _serial = CLIPBOARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ole = Ole::init();
    let (mut offer, counters) = offer(
        &[file("from-zip.txt", 3)],
        vec![Content::Stream(b"zip".to_vec())],
        true,
    );
    offer.async_mode = true;
    let object: IDataObject = offer.into();
    let source = VirtualSource::new(&object).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pasted = dir.path().to_path_buf();
    pumping(move || {
        let clipboard = read_virtual_files_from(source)
            .unwrap()
            .expect("a drop list");
        paste_into(&clipboard, &pasted);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while counters.ended.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(counters.started.load(Ordering::SeqCst), 1);
    assert_eq!(counters.ended.load(Ordering::SeqCst), 1);
    assert_eq!(*counters.end_result.lock().unwrap(), Some(S_OK));
    assert_eq!(
        std::fs::read(dir.path().join("from-zip.txt")).unwrap(),
        b"zip"
    );
}
