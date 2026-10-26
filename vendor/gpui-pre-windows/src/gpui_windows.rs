// Ferrite patches to gpui-pre-windows 0.3.9:
// - directx_devices.rs: prefer the low-power GPU; FERRITE_GPU=default|high-performance overrides.
// - window.rs: the drop target stores grfKeyState, dispatches first, returns the effect the app
//   set (None without a hint), never returns DROPEFFECT_MOVE from Drop, and exports
//   set_drop_effect, drag_key_state and drag_allowed_effects.
// - window.rs, events.rs: set_external_drag_handler registers the drag source; a posted message
//   starts it outside input dispatch, then sends FileDropEvent::Ended after the handler returns.
#![cfg(target_os = "windows")]

mod clipboard;
mod destination_list;
mod dialog;
mod direct_manipulation;
mod direct_write;
mod directx_atlas;
mod directx_devices;
mod directx_renderer;
mod dispatcher;
mod display;
mod events;
mod keyboard;
mod platform;
mod system_notifications;
mod system_settings;
mod util;
mod vsync;
mod window;
mod wrapper;

pub(crate) use clipboard::*;
pub(crate) use destination_list::*;
pub(crate) use direct_write::*;
pub(crate) use directx_atlas::*;
pub(crate) use directx_devices::*;
pub(crate) use directx_renderer::*;
pub(crate) use dispatcher::*;
pub(crate) use display::*;
pub(crate) use events::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
pub(crate) use system_notifications::*;
pub(crate) use system_settings::*;
pub(crate) use util::*;
pub(crate) use vsync::*;
pub(crate) use window::*;
pub(crate) use wrapper::*;

pub use platform::WindowsPlatform;
pub use window::{
    drag_allowed_effects, drag_key_state, set_drop_effect, set_external_drag_handler,
};

pub(crate) use windows::Win32::Foundation::HWND;
