//! Rendering helpers and GPUI `Render` implementation for [`TerminalDrawer`].

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{MouseButton, WeakEntity, canvas};

use super::super::grid::{GridStyle, TerminalGrid};
use super::super::header::{
    HEADER_WRAP_WIDTH, HeaderActions, HeaderModel, HeaderTab, render_header,
};
use super::super::session::{MissingShellInfo, SessionState, default_shell_index};
use super::super::states::{render_exit_bar, render_missing_shell};
use super::super::{grid_dimensions, measure_cell};
use super::{
    CONTEXT, CloseSession, NewSession, NextSession, OpenShellPicker, PreviousSession, SessionEntry,
    TerminalDrawer,
};
use crate::prelude::*;
use crate::primitives::IconName;
use crate::tokens;

fn defer_notify(weak: WeakEntity<TerminalDrawer>, cx: &mut App) {
    cx.defer(move |cx| {
        let _ = weak.update(cx, |_, cx| cx.notify());
    });
}

impl TerminalDrawer {
    fn header(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let width = self
            .measured_width
            .get()
            .unwrap_or_else(|| window.viewport_size().width);
        let tabs: Vec<HeaderTab> = self
            .sessions
            .iter()
            .map(|entry| {
                let session = entry.entity.read(cx);
                let is_exited = matches!(session.state(), SessionState::Exited(_));
                HeaderTab {
                    label: session.tab_label(),
                    status: session.tab_status(),
                    italic: is_exited,
                    icon: IconName::SquareTerminal,
                }
            })
            .collect();
        let active_summary = self
            .sessions
            .get(self.active)
            .map(|entry| entry.entity.read(cx).tab_label())
            .unwrap_or_default();
        let model = HeaderModel {
            tabs,
            active: self.active,
            is_focused: self.focus.is_focused(window),
            is_two_row: width < HEADER_WRAP_WIDTH,
            active_summary,
            shells: self.detected_shells.clone(),
            default_shell: default_shell_index(&self.configured_shell, &self.detected_shells),
            shell_picker: self.shell_picker.clone(),
            tabs_menu: self.tabs_menu.clone(),
        };
        let weak_select = cx.weak_entity();
        let weak_close = cx.weak_entity();
        let weak_new = cx.weak_entity();
        let weak_pick = cx.weak_entity();
        let actions = HeaderActions {
            on_select: Rc::new(move |index, window, cx| {
                let idx = *index;
                let _ = weak_select.update(cx, |this, cx| this.select_session(idx, window, cx));
            }),
            on_close: Rc::new(move |index, window, cx| {
                let idx = *index;
                let _ = weak_close.update(cx, |this, cx| this.close_session_at(idx, window, cx));
            }),
            on_new: Rc::new(move |window, cx| {
                let _ = weak_new.update(cx, |this, cx| this.new_session(window, cx));
            }),
            on_pick_shell: Rc::new(move |shell, window, cx| {
                let _ = weak_pick.update(cx, |this, cx| {
                    this.new_session_with_shell(shell, window, cx)
                });
            }),
            on_hide: Rc::clone(&self.on_hide),
        };
        render_header(model, actions, cx)
    }

    fn body(&self, mono_font: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.sessions.get(self.active) else {
            return div().flex_1().min_h_0().into_any_element();
        };
        match entry.entity.read(cx).state() {
            SessionState::MissingShell(info) => self.missing_shell_body(&info, cx),
            SessionState::Exited(code) => self.exited_body(entry, code, mono_font, cx),
            SessionState::Running => self.grid_body(entry, mono_font, cx).into_any_element(),
        }
    }

    fn missing_shell_body(&self, info: &MissingShellInfo, cx: &mut Context<Self>) -> AnyElement {
        let weak_start = cx.weak_entity();
        let weak_picker = cx.weak_entity();
        let on_fallback = Rc::new(move |window: &mut Window, cx: &mut App| {
            let _ = weak_start.update(cx, |this, cx| this.start_fallback_active(window, cx));
        });
        let on_picker = Rc::new(move |window: &mut Window, cx: &mut App| {
            let _ = weak_picker.update(cx, |this, cx| this.open_shell_picker(window, cx));
        });
        render_missing_shell(
            info,
            !self.detected_shells.is_empty(),
            on_fallback,
            on_picker,
            cx,
        )
    }

    fn exited_body(
        &self,
        entry: &SessionEntry,
        code: Option<u32>,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak_restart = cx.weak_entity();
        let weak_close = cx.weak_entity();
        let active_idx = self.active;
        let on_restart = Rc::new(move |window: &mut Window, cx: &mut App| {
            let _ = weak_restart.update(cx, |this, cx| this.restart_active(window, cx));
        });
        let on_close = Rc::new(move |window: &mut Window, cx: &mut App| {
            let _ = weak_close.update(cx, |this, cx| this.close_session_at(active_idx, window, cx));
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(self.grid_body(entry, mono_font, cx))
            .child(render_exit_bar(code, on_restart, on_close, cx))
            .into_any_element()
    }

    fn grid_body(
        &self,
        entry: &SessionEntry,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let snapshot = entry.entity.read(cx).snapshot();
        let style = GridStyle::default();
        let measured = Rc::clone(&self.measured_dims);
        let weak = cx.weak_entity();
        let font_family = style.font_family.clone().unwrap_or(mono_font);
        let font_size = style.font_size;
        let line_height = style.line_height;
        let pad = style.padding;
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(TerminalGrid::new(snapshot, entry.cache.clone()).style(style))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let metrics = measure_cell(window, &font_family, font_size, line_height);
                        let inner_w = (bounds.size.width - pad * 2.0).max(px(0.0));
                        let inner_h = (bounds.size.height - pad * 2.0).max(px(0.0));
                        let dims = grid_dimensions(inner_w, inner_h, metrics.width, metrics.height);
                        if dims.0 == 0 || dims.1 == 0 || measured.get() == dims {
                            return;
                        }
                        measured.set(dims);
                        let weak = weak.clone();
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |drawer, cx| drawer.sync_measured_size(cx));
                        });
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    fn width_tracker(&self, cx: &mut Context<Self>) -> AnyElement {
        let measured = Rc::clone(&self.measured_width);
        let weak = cx.weak_entity();
        canvas(
            move |bounds, _, cx| {
                let next = bounds.size.width;
                let prev = measured.get();
                let wrap_changed =
                    prev.is_some_and(|w| (w < HEADER_WRAP_WIDTH) != (next < HEADER_WRAP_WIDTH));
                let first_narrow = prev.is_none() && next < HEADER_WRAP_WIDTH;
                measured.set(Some(next));
                if wrap_changed || first_narrow {
                    defer_notify(weak.clone(), cx);
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}

impl Render for TerminalDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let border = if self.focus.is_focused(window) {
            t.colors.accent
        } else {
            t.colors.line
        };
        let header = self.header(window, cx);
        let body = self.body(t.typography.mono_font.clone(), cx);
        let tracker = self.width_tracker(cx);

        div()
            .id("terminal-drawer")
            .test_support()
            .aria_label(navo_l10n::tr("terminal-drawer-label"))
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(t.terminal.background)
            .border_1()
            .border_color(border)
            .rounded(t.radii.card)
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &NewSession, window, cx| {
                this.new_session(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenShellPicker, window, cx| {
                this.open_shell_picker(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseSession, window, cx| {
                this.close_session_at(this.active, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                this.next_session(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousSession, window, cx| {
                this.previous_session(window, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(tracker)
            .child(header)
            .child(body)
    }
}
