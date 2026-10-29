use gpui_kit::{Action, FocusHandle, Subscription, component::kbd::Kbd};
use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};
use rusty_core::{Command, MAX_PANES, MAX_WORKSPACES, PaneId, TabCommand, TabId};
use rusty_l10n::tr;

use super::FerriteWindow;
use super::prompt::PromptKind;
use super::shell_menu::put_paths_on_clipboard;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;

const WORKSPACES: [&str; MAX_WORKSPACES] = [
    "tab.workspace.1",
    "tab.workspace.2",
    "tab.workspace.3",
    "tab.workspace.4",
    "tab.workspace.5",
    "tab.workspace.6",
    "tab.workspace.7",
    "tab.workspace.8",
    "tab.workspace.9",
];
const PANES: [&str; MAX_PANES] = ["tab.pane.1", "tab.pane.2", "tab.pane.3", "tab.pane.4"];

fn binding_hint(action: &dyn Action, focus: &FocusHandle, window: &Window) -> Option<String> {
    let binding = window.highest_precedence_binding_for_action_in(action, focus)?;
    Some(
        binding
            .keystrokes()
            .iter()
            .map(|key| Kbd::format(key.inner()))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[derive(Default)]
pub(super) struct TabMenu(Option<(Entity<ContextMenu>, Subscription)>);

fn item(id: &'static str, label: &str, enabled: bool, hint: Option<&str>) -> MenuItem {
    let mut item = MenuItem::new(ItemId::Action(id), label, Group::Ferrite);
    item.enabled = enabled;
    item.accelerator = hint.map(str::to_owned);
    item
}

fn submenu(id: &'static str, label: &str, children: Vec<MenuItem>) -> MenuItem {
    let mut item = item(id, label, true, None);
    item.kind = ItemKind::Submenu(Submenu::Filled(children));
    item
}

#[expect(clippy::too_many_lines, reason = "menu order is kept together")]
fn tab_menu(
    state: &rusty_core::AppState,
    pane_id: PaneId,
    index: usize,
    hint: &dyn Fn(&dyn Action) -> Option<String>,
) -> Option<ShellMenu> {
    let window = state.window(state.focused_window())?;
    let pane = window.pane(pane_id)?;
    let tab = pane.tabs().get(index)?;
    let separator = || MenuItem::separator(Group::Ferrite);
    let mut workspaces: Vec<_> = window
        .workspaces()
        .iter()
        .enumerate()
        .map(|(i, workspace)| {
            item(
                WORKSPACES[i],
                &workspace.name,
                i != window.active_workspace(),
                None,
            )
        })
        .collect();
    workspaces.push(separator());
    workspaces.push(item(
        "tab.workspace.new",
        &tr("tab-menu-new-workspace"),
        window.workspaces().len() < MAX_WORKSPACES,
        None,
    ));
    let panes = window.layout().numbered();
    let mut destinations: Vec<_> = panes
        .iter()
        .enumerate()
        .map(|(i, id)| {
            item(
                PANES[i],
                &rusty_l10n::t!("tab-menu-pane-number", number = (i + 1).to_string()),
                *id != pane_id,
                None,
            )
        })
        .collect();
    destinations.push(item(
        "tab.pane.new",
        &tr("tab-menu-new-pane"),
        panes.len() < MAX_PANES,
        None,
    ));
    let others = pane
        .tabs()
        .iter()
        .any(|other| other.id() != tab.id() && !other.is_pinned());
    let right = pane
        .tabs()
        .iter()
        .skip(index + 1)
        .any(|other| !other.is_pinned());
    Some(ShellMenu {
        items: vec![
            item(
                "tab.pin",
                &tr(if tab.is_pinned() {
                    "tab-menu-unpin"
                } else {
                    "tab-menu-pin"
                }),
                true,
                None,
            ),
            separator(),
            item("tab.rename", &tr("tab-menu-rename"), true, None),
            separator(),
            item("tab.duplicate", &tr("tab-menu-duplicate"), true, None),
            item(
                "tab.clone",
                &tr("tab-menu-clone"),
                panes.len() > 1,
                hint(&super::tab_keys::CloneTab).as_deref(),
            ),
            separator(),
            submenu("tab.workspaces", &tr("tab-menu-workspaces"), workspaces),
            separator(),
            submenu("tab.panes", &tr("tab-menu-panes"), destinations),
            separator(),
            item("tab.copy", &tr("tab-menu-copy"), true, None),
            separator(),
            item(
                "tab.close",
                &tr("tab-menu-close"),
                panes.len() > 1 || pane.tabs().len() > 1,
                hint(&super::tab_keys::CloseTab).as_deref(),
            ),
            item(
                "tab.close-others",
                &tr("tab-menu-close-others"),
                others,
                None,
            ),
            item("tab.close-right", &tr("tab-menu-close-right"), right, None),
            separator(),
            item(
                "tab.reopen",
                &tr("tab-menu-reopen"),
                state.can_reopen_tab(),
                hint(&super::tab_actions::ReopenClosedTab).as_deref(),
            ),
        ],
    })
}

impl FerriteWindow {
    pub(super) fn tab_menu_view(&self) -> Option<Entity<ContextMenu>> {
        self.tab_menu.0.as_ref().map(|(view, _)| view.clone())
    }

    #[expect(clippy::too_many_arguments, reason = "GPUI event context")]
    pub(super) fn open_tab_menu(
        &mut self,
        pane: PaneId,
        index: usize,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.browser.state();
        let Some(id) = state
            .pane(pane)
            .and_then(|pane| pane.tabs().get(index))
            .map(|tab| tab.id())
        else {
            return;
        };
        let focused_pane = state.focused_pane();
        let focus = self.handle(focused_pane, cx);
        let state = self.browser.state();
        let Some(menu) = tab_menu(state, pane, index, &|action| {
            binding_hint(action, &focus, window)
        }) else {
            return;
        };
        let input = ContextMenuInput {
            menu,
            context: tr("tab-menu-title").into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(focus),
        };
        let view = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        });
        let events = cx.subscribe_in(&view, window, move |this, _, event, window, cx| {
            this.tab_menu_event(id, event, window, cx)
        });
        self.tab_menu = TabMenu(Some((view, events)));
        cx.notify();
    }

    #[expect(clippy::too_many_arguments, reason = "GPUI event context")]
    fn tab_menu_event(
        &mut self,
        id: TabId,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
            return;
        }
        self.tab_menu = TabMenu::default();
        if let ContextMenuEvent::Chosen(ItemId::Action(name)) = event {
            self.run_tab_menu(id, name, window, cx);
        }
        cx.notify();
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "GPUI event context and menu actions"
    )]
    fn run_tab_menu(&mut self, id: TabId, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.browser.state().tab(id) else {
            return;
        };
        let target = Some(id);
        let command = match name {
            "tab.pin" => Some(TabCommand::Pin {
                tab: target,
                pinned: !tab.is_pinned(),
            }),
            "tab.rename" => {
                self.open_prompt(PromptKind::RenameTab {
                    tab: id,
                    label: self.tab_title(tab).to_string(),
                });
                None
            }
            "tab.duplicate" => Some(TabCommand::Duplicate { tab: target }),
            "tab.clone" => {
                let result = self.browser.clone_tab_to_other_pane_of(id);
                self.show_refusal(result);
                None
            }
            "tab.copy" => {
                let folder = tab.base_or_location().clone();
                self.lane.push(Box::new(move || {
                    Box::new(move |_, _, cx| put_paths_on_clipboard(&[folder], cx))
                }));
                None
            }
            "tab.close" => Some(TabCommand::Close { tab: target }),
            "tab.close-others" => Some(TabCommand::CloseOthers { tab: target }),
            "tab.close-right" => Some(TabCommand::CloseRight { tab: target }),
            "tab.reopen" => Some(TabCommand::Reopen),
            "tab.workspace.new" => Some(TabCommand::MoveToNewWorkspace { tab: target }),
            "tab.pane.new" => Some(TabCommand::MoveToNewPane { tab: target }),
            _ => WORKSPACES
                .iter()
                .position(|id| *id == name)
                .map(|n| TabCommand::MoveToWorkspace {
                    tab: target,
                    n: n + 1,
                })
                .or_else(|| {
                    PANES
                        .iter()
                        .position(|id| *id == name)
                        .map(|n| TabCommand::MoveToPane {
                            tab: target,
                            n: n + 1,
                        })
                }),
        };
        if let Some(command) = command {
            self.apply(&Command::Tab(command), window, cx);
        }
    }
}

#[cfg(test)]
mod tests;
