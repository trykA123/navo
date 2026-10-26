use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use rusty_core::placement::PhysicalRect;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX, DT_WORDBREAK,
    DeleteDC, DeleteObject, DrawTextW, GdiFlush, HBITMAP, HDC, HFONT, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, DispatchMessageW, HWND_BOTTOM, MSG, MsgWaitForMultipleObjects, PM_REMOVE,
    PeekMessageW, QS_ALLINPUT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SetWindowPos, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow,
};

use super::canvas::{self, Canvas, Look};
use super::timing::{self, Event, Frame, Surface};
use super::windows_desktop::{colorref, monitor, motion, open_window, status_font, theme};

pub(super) fn run(events: &Receiver<Event>) {
    let age = crate::metrics::process_age().unwrap_or_default();
    let origin = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
    let mut native = Native::default();
    timing::drive(motion(), || origin.elapsed(), events, &mut native);
}

#[derive(Default)]
struct Native {
    near: Option<PhysicalRect>,
    window: Option<Window>,
    has_failed: bool,
}

impl Surface for Native {
    fn wait(&mut self, timeout: Duration) {
        if self.window.is_some() {
            pump(timeout);
        } else {
            std::thread::sleep(timeout);
        }
    }

    fn paint(&mut self, frame: Frame) {
        if self.window.is_none() && !self.has_failed {
            self.window = Window::create(self.near).map(|mut window| {
                window.draw(frame);
                window.show();
                window
            });
            self.has_failed = self.window.is_none();
        }
        if let Some(window) = &mut self.window {
            window.draw(frame);
        }
    }

    fn place(&mut self, near: PhysicalRect) {
        self.near = Some(near);
        if let Some(window) = &mut self.window {
            window.move_near(near);
        }
    }

    fn lower(&mut self) {
        if let Some(window) = &self.window {
            window.lower();
        }
    }
}

struct Window {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u32,
    font: HFONT,
    canvas: Canvas,
    look: Look,
    origin: POINT,
    drawn: Option<Frame>,
    slow: Vec<u16>,
}

impl Window {
    fn create(near: Option<PhysicalRect>) -> Option<Self> {
        let (work, scale) = monitor(near);
        let look = theme();
        let canvas = Canvas::new(scale, look)?;
        let origin = centre(work, &canvas);
        let hwnd = open_window(origin, canvas.width() as i32, canvas.height() as i32)?;
        let mut window = Self {
            hwnd,
            dc: null_mut(),
            bitmap: null_mut(),
            bits: null_mut(),
            font: null_mut(),
            canvas,
            look,
            origin,
            drawn: None,
            slow: Vec::new(),
        };
        window.prepare(scale).then_some(window)
    }

    fn prepare(&mut self, scale: f32) -> bool {
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: self.canvas.width() as i32,
            biHeight: -(self.canvas.height() as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..BITMAPINFO::default()
        };
        let mut bits: *mut c_void = null_mut();
        // SAFETY: `info` describes a top-down 32-bit DIB and `bits` receives its pixel pointer; the DC and bitmap are released in Drop.
        unsafe {
            self.dc = CreateCompatibleDC(null_mut());
            self.bitmap = CreateDIBSection(
                self.dc,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                null_mut(),
                0,
            );
            if self.dc.is_null() || self.bitmap.is_null() || bits.is_null() {
                return false;
            }
            SelectObject(self.dc, self.bitmap);
            self.font = status_font(scale);
            if !self.font.is_null() {
                SelectObject(self.dc, self.font);
            }
        }
        self.bits = bits.cast();
        true
    }

    fn show(&self) {
        let flags = SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER;
        // SAFETY: `hwnd` is this thread's live splash window; SWP_NOACTIVATE never takes focus.
        unsafe { SetWindowPos(self.hwnd, null_mut(), 0, 0, 0, 0, flags) };
    }

    fn move_near(&mut self, near: PhysicalRect) {
        let (work, _) = monitor(Some(near));
        self.origin = centre(work, &self.canvas);
        let flags = SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE;
        // SAFETY: `hwnd` is this thread's live splash window; SWP_NOACTIVATE never takes focus.
        unsafe {
            SetWindowPos(
                self.hwnd,
                null_mut(),
                self.origin.x,
                self.origin.y,
                0,
                0,
                flags,
            )
        };
    }

    fn lower(&self) {
        let flags = SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE;
        // SAFETY: `hwnd` is this thread's live splash window; HWND_BOTTOM only changes its z-order.
        unsafe { SetWindowPos(self.hwnd, HWND_BOTTOM, 0, 0, 0, 0, flags) };
    }

    fn draw(&mut self, frame: Frame) {
        let before = self.drawn.replace(frame);
        if before == Some(frame) {
            return;
        }
        self.canvas.set_bar(frame.bar);
        let rows = self.canvas.changed_rows(before, frame);
        let length = self.canvas.pixels().len();
        // SAFETY: GDI has finished with the DIB, which holds exactly width * height pixels, the canvas size.
        unsafe {
            GdiFlush();
            canvas::copy_rows(
                std::slice::from_raw_parts_mut(self.bits, length),
                self.canvas.pixels(),
                self.canvas.width(),
                &rows,
            );
        }
        if frame.is_slow && before.is_none_or(|before| !before.is_slow) {
            self.draw_status();
        }
        self.present();
    }

    fn draw_status(&mut self) {
        if self.slow.is_empty() {
            self.slow = rusty_l10n::tr("splash-slow").encode_utf16().collect();
        }
        let [left, top, right, bottom] = self.canvas.status();
        let mut rect = RECT {
            left,
            top,
            right,
            bottom,
        };
        let width = self.canvas.width();
        let length = self.canvas.pixels().len();
        // SAFETY: the DC owns the DIB and the font; `slow` outlives the call; the alpha fix stays inside the DIB.
        unsafe {
            SetBkMode(self.dc, TRANSPARENT as i32);
            SetTextColor(self.dc, colorref(self.look.muted));
            DrawTextW(
                self.dc,
                self.slow.as_ptr(),
                self.slow.len() as i32,
                &raw mut rect,
                DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
            );
            GdiFlush();
            canvas::make_opaque(
                std::slice::from_raw_parts_mut(self.bits, length),
                width,
                [left, top, right, bottom],
            );
        }
    }

    fn present(&self) {
        let size = SIZE {
            cx: self.canvas.width() as i32,
            cy: self.canvas.height() as i32,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: the window is layered and owned by this thread; the DC holds a premultiplied DIB of `size`.
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                null_mut(),
                &raw const self.origin,
                &raw const size,
                self.dc,
                &raw const source,
                0,
                &raw const blend,
                ULW_ALPHA,
            );
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: each handle was created by this window on this thread and is released once.
        unsafe {
            DestroyWindow(self.hwnd);
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
        }
    }
}

fn centre(work: RECT, canvas: &Canvas) -> POINT {
    let (width, height) = (canvas.width() as i32, canvas.height() as i32);
    POINT {
        x: work.left + (work.right - work.left - width) / 2,
        y: work.top + (work.bottom - work.top - height) / 2,
    }
}

fn pump(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let mut message = MSG::default();
    loop {
        // SAFETY: standard message loop on the thread that owns the splash window.
        unsafe {
            while PeekMessageW(&raw mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        let millis = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
        // SAFETY: waits on this thread's own message queue only.
        unsafe { MsgWaitForMultipleObjects(0, null(), 0, millis, QS_ALLINPUT) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_splash_window_is_created_drawn_and_destroyed_without_showing() {
        let mut window = Window::create(None).unwrap();
        window.draw(Frame {
            bar: 0.5,
            is_slow: true,
        });
        assert!(!window.hwnd.is_null());
        drop(window);
    }
}
