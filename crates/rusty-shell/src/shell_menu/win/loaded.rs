use std::collections::HashMap;
use std::path::PathBuf;

use rusty_core::shell_menu::{Group, MenuItem, ShellMenu, Submenu};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    CMF_NORMAL, CMF_SYNCCASCADEMENU, IContextMenu, IContextMenu2, IContextMenu3,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, HMENU, WM_INITMENUPOPUP,
};
use windows::core::Interface;

use super::read::{Entry, ID_FIRST, ID_LAST, Verbs, has_named_item, read_level};
use super::sta::MenuHandler;
use super::{selection, shell_error};
use crate::shell_menu::MenuError;
use crate::shell_menu::classify::{guess_group, is_unfilled, to_menu_item};

const MAX_DEPTH: usize = 4;

#[derive(Clone, Copy)]
struct SubmenuRef {
    hmenu: HMENU,
    position: u32,
    group: Group,
}

pub(super) struct LoadedMenu {
    pub(super) menu: IContextMenu,
    pub(super) hmenu: HMENU,
    pub(super) command_count: u32,
    submenus: HashMap<Vec<usize>, SubmenuRef>,
}

impl LoadedMenu {
    pub(super) fn query(paths: &[PathBuf], owner: HWND) -> Result<Self, MenuError> {
        let menu = selection::context_menu(paths, owner)?;
        // SAFETY: a fresh popup menu, owned by the returned value and destroyed in Drop.
        let hmenu = unsafe { CreatePopupMenu() }.map_err(shell_error("CreatePopupMenu"))?;
        let mut loaded = Self {
            menu,
            hmenu,
            command_count: 0,
            submenus: HashMap::new(),
        };
        let flags = CMF_NORMAL | CMF_SYNCCASCADEMENU;
        // SAFETY: a live menu handle and a valid id range.
        let result = unsafe {
            loaded
                .menu
                .QueryContextMenu(hmenu, 0, ID_FIRST, ID_LAST, flags)
        };
        result
            .ok()
            .map_err(shell_error("IContextMenu::QueryContextMenu"))?;
        loaded.command_count = u32::try_from(result.0 & 0xFFFF).unwrap_or(0);
        Ok(loaded)
    }

    pub(super) fn verbs(&self) -> Verbs<'_> {
        Verbs {
            menu: &self.menu,
            command_count: self.command_count,
        }
    }

    pub(super) fn handler(&self) -> Option<MenuHandler> {
        self.menu
            .cast::<IContextMenu3>()
            .map(MenuHandler::Menu3)
            .or_else(|_| self.menu.cast::<IContextMenu2>().map(MenuHandler::Menu2))
            .ok()
    }

    pub(super) fn walk(&mut self) -> ShellMenu {
        let level = read_level(self.hmenu, &self.verbs());
        ShellMenu {
            items: self.build(level, &[], None),
        }
    }

    pub(super) fn fill(&mut self, path: &[usize]) -> Result<Submenu, MenuError> {
        let target = self.target(path)?;
        let initialised = self.init_popup(target);
        let unfilled = if initialised {
            Submenu::Loading
        } else {
            Submenu::Failed
        };
        Ok(self.read_filled(target, path).unwrap_or(unfilled))
    }

    pub(super) fn poll_fill(&mut self, path: &[usize]) -> Option<Result<Submenu, MenuError>> {
        match self.target(path) {
            Ok(target) => self.read_filled(target, path).map(Ok),
            Err(error) => Some(Err(error)),
        }
    }

    fn target(&self, path: &[usize]) -> Result<SubmenuRef, MenuError> {
        self.submenus
            .get(path)
            .copied()
            .ok_or_else(|| MenuError::NoSubmenu(path.to_vec()))
    }

    fn read_filled(&mut self, target: SubmenuRef, path: &[usize]) -> Option<Submenu> {
        if !has_named_item(target.hmenu) {
            return None;
        }
        let level = read_level(target.hmenu, &self.verbs());
        Some(Submenu::Filled(self.build(level, path, Some(target.group))))
    }

    fn build(&mut self, level: Vec<Entry>, path: &[usize], parent: Option<Group>) -> Vec<MenuItem> {
        level
            .into_iter()
            .enumerate()
            .map(|(position, entry)| {
                let group = parent
                    .unwrap_or_else(|| guess_group(entry.raw.verb.as_deref(), &entry.raw.text));
                let submenu = entry
                    .submenu
                    .map(|hmenu| self.submenu(hmenu, [path, &[position]].concat(), group));
                to_menu_item(entry.raw, submenu, Some(group))
            })
            .collect()
    }

    fn submenu(&mut self, hmenu: HMENU, child_path: Vec<usize>, group: Group) -> Submenu {
        let position = child_path.last().copied().unwrap_or(0);
        let reference = SubmenuRef {
            hmenu,
            position: u32::try_from(position).unwrap_or(u32::MAX),
            group,
        };
        self.submenus.insert(child_path.clone(), reference);
        let level = read_level(hmenu, &self.verbs());
        let raws: Vec<_> = level.iter().map(|entry| entry.raw.clone()).collect();
        if child_path.len() > MAX_DEPTH || is_unfilled(&raws) {
            return Submenu::Lazy;
        }
        Submenu::Filled(self.build(level, &child_path, Some(group)))
    }

    fn init_popup(&self, target: SubmenuRef) -> bool {
        let wparam = WPARAM(target.hmenu.0 as usize);
        let lparam = LPARAM(isize::try_from(target.position).unwrap_or(0));
        let mut result = LRESULT(0);
        // SAFETY: WM_INITMENUPOPUP with a live submenu handle, as a menu loop would send it.
        let outcome = unsafe {
            match self.handler() {
                Some(MenuHandler::Menu3(menu)) => {
                    menu.HandleMenuMsg2(WM_INITMENUPOPUP, wparam, lparam, Some(&mut result))
                }
                Some(MenuHandler::Menu2(menu)) => {
                    menu.HandleMenuMsg(WM_INITMENUPOPUP, wparam, lparam)
                }
                None => return false,
            }
        };
        outcome.is_ok()
    }
}

impl Drop for LoadedMenu {
    fn drop(&mut self) {
        // SAFETY: the menu came from CreatePopupMenu and is destroyed once, with its submenus.
        let _ = unsafe { DestroyMenu(self.hmenu) };
    }
}
