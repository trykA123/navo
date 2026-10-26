use rusty_core::shell_menu::Icon;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, GetDIBits, GetObjectW, HBITMAP, HDC,
    ReleaseDC, SelectObject,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODA_DRAWENTIRE, ODT_MENU};
use windows::Win32::UI::WindowsAndMessaging::{
    HBMMENU_CALLBACK, HMENU, MENUITEMINFOW, MFT_OWNERDRAW, WM_DRAWITEM, WM_MEASUREITEM,
};

use super::sta::MenuHandler;
use crate::shell_menu::pixels::{icon_from_bgra, icon_from_two, icon_size};

pub(super) fn of_item(item: &MENUITEMINFOW) -> Option<Icon> {
    [item.hbmpItem, item.hbmpUnchecked]
        .into_iter()
        .find(|bitmap| is_bitmap(*bitmap))
        .and_then(to_icon)
}

fn is_bitmap(bitmap: HBITMAP) -> bool {
    !(-1..=11).contains(&(bitmap.0 as isize))
}

fn to_icon(bitmap: HBITMAP) -> Option<Icon> {
    let mut bm = BITMAP::default();
    let size = i32::try_from(size_of::<BITMAP>()).ok()?;
    // SAFETY: bm is a BITMAP-sized out buffer for this HBITMAP.
    if unsafe { GetObjectW(bitmap.into(), size, Some((&raw mut bm).cast())) } == 0 {
        return None;
    }
    let (width, height) = icon_size(bm.bmWidth, bm.bmHeight)?;
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: bm.bmWidth,
            biHeight: -i32::try_from(height).ok()?,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut data = vec![0u8; usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?];
    // SAFETY: data holds width * height 32-bit pixels as the header asks; the screen DC is released.
    let lines = unsafe {
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(data.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        lines
    };
    (lines != 0)
        .then(|| icon_from_bgra(width, height, data))
        .flatten()
}

const DRAWN_SIDE: i32 = 16;

pub(super) fn is_owner_drawn(item: &MENUITEMINFOW) -> bool {
    item.hbmpItem == HBMMENU_CALLBACK || item.fType.contains(MFT_OWNERDRAW)
}

/// Asks the extension to draw the item into a memory DC, once on black and once on white.
pub(super) fn drawn(handler: &MenuHandler, hmenu: HMENU, item: &MENUITEMINFOW) -> Option<Icon> {
    let mut measure = MEASUREITEMSTRUCT {
        CtlType: ODT_MENU,
        itemID: item.wID,
        itemData: item.dwItemData,
        ..Default::default()
    };
    // SAFETY: WM_MEASUREITEM points at a MEASUREITEMSTRUCT that lives across the call.
    unsafe {
        handler.send(
            WM_MEASUREITEM,
            WPARAM(0),
            LPARAM((&raw mut measure) as isize),
        )?;
    }
    let on_black = draw_on(handler, hmenu, item, 0x00)?;
    let on_white = draw_on(handler, hmenu, item, 0xFF)?;
    let side = DRAWN_SIDE.unsigned_abs();
    icon_from_two(side, side, &on_black, &on_white)
}

fn dib_info() -> Option<BITMAPINFO> {
    Some(BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: DRAWN_SIDE,
            biHeight: -DRAWN_SIDE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    })
}

fn draw_request(hmenu: HMENU, item: &MENUITEMINFOW, dc: HDC) -> DRAWITEMSTRUCT {
    DRAWITEMSTRUCT {
        CtlType: ODT_MENU,
        itemID: item.wID,
        itemAction: ODA_DRAWENTIRE,
        hwndItem: HWND(hmenu.0),
        hDC: dc,
        rcItem: RECT {
            left: 0,
            top: 0,
            right: DRAWN_SIDE,
            bottom: DRAWN_SIDE,
        },
        itemData: item.dwItemData,
        ..Default::default()
    }
}

fn draw_on(
    handler: &MenuHandler,
    hmenu: HMENU,
    item: &MENUITEMINFOW,
    background: u8,
) -> Option<Vec<u8>> {
    let len = usize::try_from(DRAWN_SIDE * DRAWN_SIDE * 4).ok()?;
    let info = dib_info()?;
    // SAFETY: a memory DC with a 32-bit DIB section of `len` bytes; every GDI object is released before returning.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Some(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .ok()
            .filter(|_| !bits.is_null())
        else {
            let _ = DeleteDC(dc);
            return None;
        };
        let previous = SelectObject(dc, bitmap.into());
        std::ptr::write_bytes(bits.cast::<u8>(), background, len);
        let mut draw = draw_request(hmenu, item, dc);
        let painted = handler
            .send(WM_DRAWITEM, WPARAM(0), LPARAM((&raw mut draw) as isize))
            .is_some();
        let _ = GdiFlush();
        let pixels = painted.then(|| std::slice::from_raw_parts(bits.cast::<u8>(), len).to_vec());
        let _ = SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        pixels
    }
}
