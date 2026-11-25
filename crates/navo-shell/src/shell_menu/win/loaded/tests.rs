//! Exercises lazy loading with a static Win32 submenu and no popup message handler.

use navo_core::shell_menu::ItemKind;
use windows::Win32::Foundation::E_NOTIMPL;
use windows::Win32::UI::Shell::{CMINVOKECOMMANDINFO, IContextMenu_Impl};
use windows::Win32::UI::WindowsAndMessaging::{AppendMenuW, MF_POPUP, MF_STRING};
use windows::core::{HRESULT, PSTR, Result, implement, w};

use super::{CreatePopupMenu, DestroyMenu, HMENU, IContextMenu, LoadedMenu, MenuPaths, Submenu};

#[implement(IContextMenu)]
struct StaticMenu;

impl IContextMenu_Impl for StaticMenu_Impl {
    fn QueryContextMenu(&self, _: HMENU, _: u32, _: u32, _: u32, _: u32) -> HRESULT {
        E_NOTIMPL
    }

    fn InvokeCommand(&self, _: *const CMINVOKECOMMANDINFO) -> Result<()> {
        E_NOTIMPL.ok()
    }

    fn GetCommandString(&self, _: usize, _: u32, _: *const u32, _: PSTR, _: u32) -> Result<()> {
        E_NOTIMPL.ok()
    }
}

#[test]
fn a_static_submenu_is_lazy_then_fills_without_a_popup_handler() -> Result<()> {
    // SAFETY: LoadedMenu takes ownership of the fresh root popup.
    let hmenu = unsafe { CreatePopupMenu()? };
    let mut loaded = LoadedMenu {
        menu: StaticMenu.into(),
        hmenu,
        command_count: 0,
        paths: MenuPaths::Items(Vec::new()),
        submenus: Default::default(),
    };
    // SAFETY: the new child is either attached to the owned root or destroyed on failure.
    unsafe {
        let child = CreatePopupMenu()?;
        if let Err(error) = AppendMenuW(hmenu, MF_POPUP, child.0 as usize, w!("Archive")) {
            let _ = DestroyMenu(child);
            return Err(error);
        }
        AppendMenuW(child, MF_STRING, 1, w!("Extract"))?;
    }

    assert!(loaded.handler().is_none());
    assert_eq!(
        loaded.walk().items[0].kind,
        ItemKind::Submenu(Submenu::Lazy)
    );
    let filled = loaded.fill(&[0]);
    assert!(matches!(filled, Ok(Submenu::Filled(items)) if items[0].text == "Extract"));
    Ok(())
}
