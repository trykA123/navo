use rusty_core::shell_menu::Icon;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, GetDIBits, GetObjectW,
    HBITMAP, ReleaseDC,
};
use windows::Win32::UI::WindowsAndMessaging::MENUITEMINFOW;

use crate::shell_menu::pixels::{icon_from_bgra, icon_size};

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
