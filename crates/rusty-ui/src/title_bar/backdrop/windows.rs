use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

pub(super) fn windows_build() -> Option<u32> {
    // SAFETY: OSVERSIONINFOW is plain data, so all zero bytes are a valid value.
    let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    info.dwOSVersionInfoSize = u32::try_from(size_of::<OSVERSIONINFOW>()).ok()?;
    // SAFETY: `info` is a valid OSVERSIONINFOW with its size field set.
    let status = unsafe { RtlGetVersion(&raw mut info) };
    (status >= 0).then_some(info.dwBuildNumber)
}
