use super::ProbeError;

pub(super) const WM_CLOSE: u32 = 0x0010;
pub(super) const WM_KEYDOWN: u32 = 0x0100;
pub(super) const WM_KEYUP: u32 = 0x0101;

/// One ASCII letter or digit, sent as its virtual key without modifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Letter(u8);

impl Letter {
    pub fn new(ch: char) -> Option<Self> {
        ch.is_ascii_alphanumeric()
            .then(|| u8::try_from(ch.to_ascii_uppercase()).ok())
            .flatten()
            .map(Self)
    }
}

/// The only keys the tool posts: none of them starts a write in Navo's file list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Down,
    PageDown,
    Escape,
    Letter(Letter),
}

impl Key {
    /// Virtual-key code and whether the key is an extended key.
    pub(super) fn virtual_key(self) -> (u16, bool) {
        match self {
            Self::Down => (0x28, true),
            Self::PageDown => (0x22, true),
            Self::Escape => (0x1B, false),
            Self::Letter(Letter(vk)) => (u16::from(vk), false),
        }
    }
}

/// A top-level window handle as an integer; meaningful only with the PID it was found with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WindowId(pub(super) isize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WindowInfo {
    pub(super) id: WindowId,
    pub(super) pid: u32,
    pub(super) visible: bool,
    pub(super) owned: bool,
}

/// The window keys go to, and the process that must still own it at every post.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Target {
    pub(super) window: WindowId,
    pub(super) pid: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Message {
    pub(super) id: u32,
    pub(super) wparam: usize,
    pub(super) lparam: isize,
}

/// Window queries and posting; Win32 on Windows, a fake in tests.
pub(super) trait WindowOs {
    fn window_pid(&self, window: WindowId) -> Option<u32>;
    fn post(&self, window: WindowId, message: Message) -> Result<(), ProbeError>;
    /// Hardware scan code for a virtual key in the current layout.
    fn scan_code(&self, virtual_key: u16) -> u8;
}

/// The first visible, unowned top-level window of `pid`, in z-order.
pub(super) fn main_window(windows: &[WindowInfo], pid: u32) -> Option<WindowId> {
    windows
        .iter()
        .find(|info| info.pid == pid && info.visible && !info.owned)
        .map(|info| info.id)
}

/// Posts one message after checking that the window still belongs to the target process.
pub(super) fn post_verified(
    os: &impl WindowOs,
    target: Target,
    message: Message,
) -> Result<(), ProbeError> {
    if os.window_pid(target.window) != Some(target.pid) {
        return Err(ProbeError::WrongProcess);
    }
    os.post(target.window, message)
}

/// Posts key-down then key-up for each key, verifying the window's PID before every post.
pub(super) fn post_keys(
    os: &impl WindowOs,
    target: Target,
    keys: &[Key],
) -> Result<(), ProbeError> {
    for key in keys {
        let (vk, extended) = key.virtual_key();
        let scan = os.scan_code(vk);
        for up in [false, true] {
            let message = Message {
                id: if up { WM_KEYUP } else { WM_KEYDOWN },
                wparam: usize::from(vk),
                lparam: key_lparam(scan, extended, up),
            };
            post_verified(os, target, message)?;
        }
    }
    Ok(())
}

/// `WM_KEYDOWN`/`WM_KEYUP` lParam: repeat 1, scan code, extended bit, previous state and
/// transition bits set for key-up.
pub(super) fn key_lparam(scan: u8, extended: bool, up: bool) -> isize {
    let mut bits = 1_u32 | (u32::from(scan) << 16);
    if extended {
        bits |= 1 << 24;
    }
    if up {
        bits |= (1 << 30) | (1 << 31);
    }
    // LPARAM is signed; Win32 reads these bits back as a 32-bit value.
    i32::from_ne_bytes(bits.to_ne_bytes()) as isize
}
