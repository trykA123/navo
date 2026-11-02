use std::path::PathBuf;

/// What one clipboard change held: text first, else a file list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureContent {
    Text(String),
    Files { paths: Vec<PathBuf>, cut: bool },
}

/// One recorded clipboard change; `owner_exe` is the owner's file stem, best effort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCapture {
    pub content: CaptureContent,
    pub owner_exe: Option<String>,
    pub sequence: u32,
}

impl CaptureContent {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Files { .. } => "files",
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Text(text) => text.chars().count(),
            Self::Files { paths, .. } => paths.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `CanIncludeInClipboardHistory` data: absent admits; anything but a nonzero DWORD refuses.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows watcher reads it")
)]
pub(super) fn admits_history(can_include: Option<&[u8]>) -> bool {
    let Some(bytes) = can_include else {
        return true;
    };
    bytes
        .get(..4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .is_some_and(|value| u32::from_le_bytes(value) != 0)
}

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows watcher reads it")
)]
pub(super) fn text_from_bytes(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows watcher reads it")
)]
pub(super) fn exe_stem(image: &[u16]) -> Option<String> {
    let image = String::from_utf16_lossy(image);
    let name = image.rsplit(['\\', '/']).next()?;
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    };
    (!stem.is_empty()).then(|| stem.to_owned())
}
