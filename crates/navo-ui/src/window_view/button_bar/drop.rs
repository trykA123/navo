//! Drag-and-drop hover feedback and drop dispatch for button bar items.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    Anchor, AnyElement, Context, DragMoveEvent, Entity, ExternalPaths, InteractiveElement,
    IntoElement, ParentElement, Styled, Window, anchored, deferred, div, point, px,
};
use navo_core::{Location, ProviderPath};

use super::fit::BarMetrics;
use super::{ButtonAction, ButtonItem};
use crate::drag::FileDrag;
use crate::file_list::DropPayload;
use crate::tokens::Theme;
use crate::window_view::drop::DragSession;
use crate::window_view::{NavoWindow, UserCommandInfo};

/// Opacity applied to buttons that do not accept files while a drag hovers the bar.
pub(super) const NON_TAKER_OPACITY: f32 = 0.4;

/// Snapshot of active button-bar drag state used while rendering the strip.
#[derive(Clone, Copy, Default)]
pub(super) struct DragVisual {
    pub(super) active: bool,
    pub(super) hovered: Option<usize>,
    pub(super) count: usize,
}

/// Returns `true` when `item` accepts dropped files according to live `user_commands` metadata.
#[must_use]
pub(crate) fn item_takes_files(item: &ButtonItem, user_commands: &[UserCommandInfo]) -> bool {
    match &item.action {
        ButtonAction::Program { takes_files, .. } => *takes_files,
        ButtonAction::User(name) => user_commands
            .iter()
            .any(|cmd| cmd.name == *name && cmd.takes_files),
        ButtonAction::Navo(_) | ButtonAction::Menu(_) | ButtonAction::Separator => false,
    }
}

/// Builds the calm anchored chip shown beneath the hovered file-taking button.
pub(super) fn drop_chip(name: &str, count: usize, m: BarMetrics, t: &Theme) -> AnyElement {
    let label = navo_l10n::t!("button-bar-run-with", name = name, count = count);
    let chip = div()
        .id("button-bar-drop-chip")
        .test_support()
        .px(t.spacing.sm)
        .py(t.spacing.xs)
        .rounded(t.radii.card)
        .bg(t.surfaces.card)
        .border_1()
        .border_color(t.surfaces.card_ring)
        .shadow(t.shadows.menu.clone())
        .text_size(t.typography.small)
        .text_color(t.colors.text)
        .whitespace_nowrap()
        .child(label);
    deferred(
        anchored()
            .anchor(Anchor::TopLeft)
            .offset(point(px(0.), m.height))
            .snap_to_window_with_margin(px(8.))
            .child(chip),
    )
    .with_priority(2)
    .into_any_element()
}

/// Attaches drag-hover and drop listeners to a file-taking button element.
pub(super) fn wire_button_drop<E: InteractiveElement>(
    button: E,
    index: usize,
    item: &ButtonItem,
    view: Entity<NavoWindow>,
) -> E {
    let button = wire_hover::<FileDrag, _>(button, index, view.clone());
    let button = wire_hover::<ExternalPaths, _>(button, index, view.clone());
    let (internal_view, internal_item) = (view.clone(), item.clone());
    let external_item = item.clone();
    button
        .on_drop(move |drag: &FileDrag, window, cx| {
            let payload = DropPayload::Internal(drag.clone());
            internal_view.update(cx, |view, cx| {
                view.drop_on_button(&internal_item, payload, window, cx);
            });
        })
        .on_drop(move |paths: &ExternalPaths, window, cx| {
            let payload = DropPayload::External(paths.clone());
            view.update(cx, |view, cx| {
                view.drop_on_button(&external_item, payload, window, cx);
            });
        })
}

fn wire_hover<T: 'static, E: InteractiveElement>(
    el: E,
    index: usize,
    view: Entity<NavoWindow>,
) -> E {
    el.on_drag_move(move |event: &DragMoveEvent<T>, _, cx| {
        let inside = event.bounds.contains(&event.event.position);
        view.update(cx, |view, cx| {
            view.set_button_drag_hover(index, inside, cx);
        });
    })
}

fn wire_bar_move<T: 'static, E: InteractiveElement>(el: E, view: Entity<NavoWindow>) -> E {
    el.on_drag_move(move |event: &DragMoveEvent<T>, _, cx| {
        let inside = event.bounds.contains(&event.event.position);
        view.update(cx, |view, cx| view.set_bar_drag_over(inside, cx));
    })
}

impl NavoWindow {
    /// Returns the active drag visual state for the button bar strip.
    pub(super) fn button_bar_drag_visual(&self) -> DragVisual {
        let active = self.drag.session.is_some() && self.button_bar.drag_over_bar;
        if !active {
            return DragVisual::default();
        }
        DragVisual {
            active: true,
            hovered: self.button_bar.drag_hover_item,
            count: self.drag_file_count(),
        }
    }

    /// Wires bar-level drag-move listeners so the strip highlights takers while hovered.
    pub(super) fn wire_bar_drag<E: InteractiveElement>(&self, bar: E, cx: &mut Context<Self>) -> E {
        let view = cx.entity();
        let bar = wire_bar_move::<FileDrag, _>(bar, view.clone());
        wire_bar_move::<ExternalPaths, _>(bar, view)
    }

    fn drag_file_count(&self) -> usize {
        let Some(session) = &self.drag.session else {
            return 0;
        };
        if !session.carried.sources.is_empty() {
            session.carried.sources.len()
        } else {
            session.external.as_ref().map_or(0, |ext| ext.0.len())
        }
    }

    fn set_bar_drag_over(&mut self, inside: bool, cx: &mut Context<Self>) {
        if self.button_bar.drag_over_bar == inside {
            return;
        }
        self.button_bar.drag_over_bar = inside;
        if !inside {
            self.button_bar.drag_hover_item = None;
        }
        cx.notify();
    }

    fn set_button_drag_hover(&mut self, index: usize, inside: bool, cx: &mut Context<Self>) {
        let changed = if inside {
            let was = self.button_bar.drag_hover_item;
            self.button_bar.drag_over_bar = true;
            self.button_bar.drag_hover_item = Some(index);
            was != Some(index)
        } else if self.button_bar.drag_hover_item == Some(index) {
            self.button_bar.drag_hover_item = None;
            true
        } else {
            false
        };
        if changed {
            cx.notify();
        }
    }

    /// Handles a file drop onto `item`, running it with the dropped locations when it takes files.
    pub(crate) fn drop_on_button(
        &mut self,
        item: &ButtonItem,
        payload: DropPayload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.button_bar.drag_over_bar = false;
        self.button_bar.drag_hover_item = None;
        if self.drops_blocked() || !item_takes_files(item, self.user_command_list()) {
            cx.notify();
            return;
        }
        let paths = match &payload {
            DropPayload::Internal(_) => {
                let Some(session) = self.take_drag(&payload) else {
                    cx.notify();
                    return;
                };
                self.set_drop_effect(None, window);
                session.carried.sources
            }
            DropPayload::External(external) => {
                let session = self.take_drag(&payload);
                self.set_drop_effect(None, window);
                self.external_button_locations(external, session)
            }
        };
        if !paths.is_empty() {
            self.run_button(item, Some(paths), window, cx);
        }
        cx.notify();
    }

    fn external_button_locations(
        &self,
        external: &ExternalPaths,
        session: Option<DragSession>,
    ) -> Vec<Location> {
        if let Some(session) = session {
            if !session.carried.sources.is_empty() {
                return session.carried.sources;
            }
            if self.drag.platform.is_some() && session.error.is_some() {
                return Vec::new();
            }
        }
        external
            .0
            .iter()
            .filter_map(|path| {
                Location::local(ProviderPath::from_text(&path.to_string_lossy())).ok()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_view::{BarIcon, ProgramId};

    fn item(action: ButtonAction) -> ButtonItem {
        ButtonItem {
            name: "Test".to_owned(),
            icon: BarIcon::Missing { dashed: false },
            action,
            line: 1,
        }
    }

    #[test]
    fn item_takes_files_matches_action_and_user_command_metadata() {
        let cmds = vec![
            UserCommandInfo {
                name: "Hash".to_owned(),
                key: None,
                takes_files: true,
            },
            UserCommandInfo {
                name: "Status".to_owned(),
                key: None,
                takes_files: false,
            },
        ];
        assert!(item_takes_files(
            &item(ButtonAction::Program {
                id: ProgramId {
                    generation: 1,
                    index: 0,
                },
                line: "sha256sum {file}".to_owned(),
                takes_files: true,
            }),
            &cmds,
        ));
        assert!(!item_takes_files(
            &item(ButtonAction::Program {
                id: ProgramId {
                    generation: 1,
                    index: 1,
                },
                line: "ping".to_owned(),
                takes_files: false,
            }),
            &cmds,
        ));
        assert!(item_takes_files(
            &item(ButtonAction::User("Hash".to_owned())),
            &cmds,
        ));
        assert!(!item_takes_files(
            &item(ButtonAction::User("Status".to_owned())),
            &cmds,
        ));
        assert!(!item_takes_files(
            &item(ButtonAction::User("Missing".to_owned())),
            &cmds,
        ));
        assert!(!item_takes_files(
            &item(ButtonAction::Navo("pane.switch".to_owned())),
            &cmds,
        ));
        assert!(!item_takes_files(&item(ButtonAction::Menu(vec![])), &cmds));
        assert!(!item_takes_files(&item(ButtonAction::Separator), &cmds));
    }
}
