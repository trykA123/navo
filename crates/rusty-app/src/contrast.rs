use rusty_ui::tokens::SystemPalette;

#[cfg(windows)]
pub fn system_palette() -> Option<SystemPalette> {
    use windows_sys::Win32::Graphics::Gdi::{
        COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
        COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
    };
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW};

    let mut contrast = HIGHCONTRASTW {
        cbSize: u32::try_from(std::mem::size_of::<HIGHCONTRASTW>()).ok()?,
        ..Default::default()
    };
    // SAFETY: `contrast` is a writable HIGHCONTRASTW whose cbSize matches its size.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            std::ptr::from_mut(&mut contrast).cast(),
            0,
        )
    };
    if read == 0 || contrast.dwFlags & HCF_HIGHCONTRASTON == 0 {
        return None;
    }
    let color = |index: SYS_COLOR_INDEX| {
        // SAFETY: GetSysColor only reads a predefined system colour index.
        rusty_ui::tokens::from_colorref(unsafe { GetSysColor(index) })
    };
    Some(SystemPalette {
        window: color(COLOR_WINDOW),
        window_text: color(COLOR_WINDOWTEXT),
        highlight: color(COLOR_HIGHLIGHT),
        highlight_text: color(COLOR_HIGHLIGHTTEXT),
        button_face: color(COLOR_BTNFACE),
        button_text: color(COLOR_BTNTEXT),
        gray_text: color(COLOR_GRAYTEXT),
        hot_light: color(COLOR_HOTLIGHT),
    })
}

#[cfg(not(windows))]
pub fn system_palette() -> Option<SystemPalette> {
    None
}
