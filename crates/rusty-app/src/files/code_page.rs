use encoding_rs::Encoding;

#[cfg(windows)]
pub fn ansi() -> &'static Encoding {
    // SAFETY: GetACP takes no arguments and only reads the process's active code page.
    let code_page = unsafe { windows_sys::Win32::Globalization::GetACP() };
    rusty_fs::encoding_for_code_page(code_page)
}

#[cfg(not(windows))]
pub fn ansi() -> &'static Encoding {
    rusty_fs::ansi_code_page()
}
