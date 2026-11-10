//! The operating system Navo was built for, used where GPUI 0.3.12 asks for one when parsing,
//! writing or showing a keystroke.
use gpui_kit::{Keystroke, OperatingSystem};

/// The build target's OS; matches the compile-time choice GPUI made itself before 0.3.12.
pub(crate) const HOST_OS: OperatingSystem = if cfg!(target_os = "windows") {
    OperatingSystem::Windows
} else if cfg!(target_os = "macos") {
    OperatingSystem::Mac
} else if cfg!(target_os = "freebsd") {
    OperatingSystem::FreeBsd
} else {
    OperatingSystem::Linux
};

/// Parses one keystroke such as `ctrl-shift-a`; `secondary-` resolves for the host OS.
/// Fails when the text is not a keystroke.
pub(crate) fn parse_keystroke(text: &str) -> Result<Keystroke, gpui_kit::InvalidKeystrokeError> {
    Keystroke::parse_with_os(text, HOST_OS)
}
