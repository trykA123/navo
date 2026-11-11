//! The View menu of the file diff and the Stream (Alt+V, D58): Simple / Rich / Custom, Unified /
//! Side by side and the parts to show. Choosing an entry runs its action; the root handles it.

use gpui_kit::{Action, Bounds, FocusHandle, Stateful, Subscription, point};
use navo_core::shell_menu::ItemId;
use navo_l10n::tr;

use super::look::{Look, Preset};
use crate::commands::compare::{ToggleWrap, ViewMenu};
use crate::compare_diff::HexAlign;
use crate::compare_diff::Layout;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::tokens;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::{MenuEntry, ViewCustom, ViewRich, ViewSimple, viewer_menu};

gpui_kit::actions!(
    compare_view,
    [
        ShowUnified,
        ShowSideBySide,
        ToggleWords,
        ToggleMap,
        AlignOffset,
        AlignContent
    ]
);

/// An open View menu and its event subscription.
pub(super) type MenuSlot = Option<(Entity<ContextMenu>, Subscription)>;

/// The look, focus and menu of the top screen: the file diff, else the Stream.
pub(super) struct LookHost<'a> {
    pub(super) look: &'a mut Look,
    pub(super) focus: &'a FocusHandle,
    pub(super) menu: &'a mut MenuSlot,
}

/// A menu entry that runs `action`; labelled by the Fluent `id` and checked when `on`.
fn run(id: &str, action: Box<dyn Action>, on: bool) -> MenuEntry {
    MenuEntry::Run(tr(id).into(), action, on)
}

/// The hex tail of the menu for an open binary pair: a break and the two align entries.
fn hex_entries(align: HexAlign) -> Vec<MenuEntry> {
    vec![
        MenuEntry::Break,
        run(
            "compare-hex-align-offset",
            Box::new(AlignOffset),
            align == HexAlign::Offset,
        ),
        run(
            "compare-hex-align-content",
            Box::new(AlignContent),
            align == HexAlign::Content,
        ),
    ]
}

/// The entries of the menu for `look`; `layout` is the layout in use, `hex` the alignment
/// when the open file diff is a binary pair, and `text` whether it shows the text file diff.
pub(in crate::window_view) fn entries(
    look: &Look,
    layout: Layout,
    hex: Option<HexAlign>,
    text: bool,
) -> Vec<MenuEntry> {
    let mut items = vec![
        run(
            "compare-view-simple",
            Box::new(ViewSimple),
            look.preset == Preset::Simple,
        ),
        run(
            "compare-view-rich",
            Box::new(ViewRich),
            look.preset == Preset::Rich,
        ),
        run(
            "compare-view-custom",
            Box::new(ViewCustom),
            look.preset == Preset::Custom,
        ),
        MenuEntry::Break,
        run(
            "compare-view-unified",
            Box::new(ShowUnified),
            layout == Layout::Unified,
        ),
        run(
            "compare-view-side-by-side",
            Box::new(ShowSideBySide),
            layout == Layout::SideBySide,
        ),
        MenuEntry::Break,
        run("compare-view-words", Box::new(ToggleWords), look.words),
        run("compare-view-map", Box::new(ToggleMap), look.map),
    ];
    if text {
        items.push(run("compare-view-wrap", Box::new(ToggleWrap), look.wrap));
    }
    if let Some(align) = hex {
        items.extend(hex_entries(align));
    }
    items
}

impl NavoWindow {
    /// Registers the actions the View menu runs on the file diff or the Stream root.
    pub(super) fn look_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ViewMenu, window, cx| {
            view.open_look_menu(window, cx);
        }))
        .on_action(cx.listener(|view, _: &ViewSimple, _, cx| {
            view.change_look(|look| look.choose(Preset::Simple), cx);
        }))
        .on_action(cx.listener(|view, _: &ViewRich, _, cx| {
            view.change_look(|look| look.choose(Preset::Rich), cx);
        }))
        .on_action(cx.listener(|view, _: &ViewCustom, _, cx| {
            view.change_look(|look| look.choose(Preset::Custom), cx);
        }))
        .on_action(cx.listener(|view, _: &ShowUnified, _, cx| {
            view.change_look(|look| look.layout = Some(Layout::Unified), cx);
        }))
        .on_action(cx.listener(|view, _: &ShowSideBySide, _, cx| {
            view.change_look(|look| look.layout = Some(Layout::SideBySide), cx);
        }))
        .on_action(cx.listener(|view, _: &ToggleWords, _, cx| {
            view.change_look(Look::toggle_words, cx);
        }))
        .on_action(cx.listener(|view, _: &ToggleMap, _, cx| {
            view.change_look(Look::toggle_map, cx);
        }))
        .on_action(cx.listener(|view, _: &AlignOffset, _, cx| {
            view.set_hex_align(HexAlign::Offset, cx);
        }))
        .on_action(cx.listener(|view, _: &AlignContent, _, cx| {
            view.set_hex_align(HexAlign::Content, cx);
        }))
    }

    /// Registers the wrap toggle on the file diff's root, where the View actions run with the
    /// `CompareView` key context; the Stream root does not handle it yet (W8k).
    pub(super) fn wrap_action(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ToggleWrap, _, cx| {
            view.change_look(Look::toggle_wrap, cx);
        }))
    }

    /// Switches the open hex body's alignment and asks for regions in that alignment.
    fn set_hex_align(&mut self, align: HexAlign, cx: &mut Context<Self>) {
        let Some(file) = self
            .compare
            .screen
            .as_mut()
            .and_then(|screen| screen.file.as_mut())
        else {
            return;
        };
        let Some(hex) = file.hex.as_mut() else {
            return;
        };
        if !hex.set_align(align) {
            return;
        }
        let run = hex.request_regions();
        if let Some(run) = run {
            file.control.byte_regions(align, run);
        }
        cx.notify();
    }

    /// The look, focus and menu slot of the file diff, else of the Stream.
    pub(super) fn look_host(&mut self) -> Option<LookHost<'_>> {
        let screen = self.compare.screen.as_mut()?;
        if let Some(file) = screen.file.as_mut() {
            let (look, focus, menu) = (&mut file.look, &file.focus, &mut file.menu);
            return Some(LookHost { look, focus, menu });
        }
        let stream = screen.stream.as_mut()?;
        let (look, focus, menu) = (&mut stream.look, &stream.focus, &mut stream.menu);
        Some(LookHost { look, focus, menu })
    }

    /// Applies `change` to the look of the top screen; a file diff gets the words setting.
    fn change_look(&mut self, change: impl FnOnce(&mut Look), cx: &mut Context<Self>) {
        let Some(host) = self.look_host() else {
            return;
        };
        change(host.look);
        if let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut())
            && let Some(view) = file.view.as_mut()
        {
            view.set_words(file.look.words);
        }
        if let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) {
            file.follow_layout();
        }
        cx.notify();
    }

    /// Opens the View menu under the header, at the right edge.
    fn open_look_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = tokens::current(cx).clone();
        let hex = self
            .compare
            .screen
            .as_ref()
            .and_then(|screen| screen.file.as_ref())
            .and_then(|file| file.hex.as_ref())
            .map(super::hex::HexView::align);
        let text = self
            .compare
            .screen
            .as_ref()
            .and_then(|screen| screen.file.as_ref())
            .is_some_and(|file| file.view.is_some() && file.hex.is_none());
        let switch = (self.compare.screen.as_ref())
            .and_then(|screen| screen.file.as_ref())
            .map(super::switch::entries);
        let Some(host) = self.look_host() else {
            return;
        };
        let mut items = entries(host.look, host.look.layout_in(window), hex, text);
        items.extend(switch.unwrap_or_default());
        let anchor = point(
            window.viewport_size().width - t.spacing.xl,
            t.spacing.path_height * 2.,
        );
        self.show_menu((items, "compare-view-title", anchor), window, cx);
    }

    /// Shows `items` titled by the Fluent id `title` in the top screen's menu slot at `anchor`.
    pub(super) fn show_menu(
        &mut self,
        (items, title, anchor): (Vec<MenuEntry>, &str, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.look_host() else {
            return;
        };
        let input = ContextMenuInput {
            menu: viewer_menu(&items, host.focus, window),
            context: tr(title).into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(host.focus.clone()),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.look_menu_event(event, window, cx);
        });
        *host.menu = Some((menu, events));
        cx.notify();
    }

    /// Closes the menu; a chosen entry then runs as an action on the screen it opened on.
    fn look_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
            return;
        }
        let Some(host) = self.look_host() else {
            return;
        };
        *host.menu = None;
        let focus = host.focus.clone();
        cx.notify();
        let ContextMenuEvent::Chosen(ItemId::Action(name)) = event else {
            return window.focus(&focus, cx);
        };
        let Ok(action) = cx.build_action(name, None) else {
            return window.focus(&focus, cx);
        };
        window.defer(cx, move |window, cx| {
            window.focus(&focus, cx);
            focus.dispatch_action(action.as_ref(), window, cx);
        });
    }

    /// The open View menu of the file diff or the Stream, for the window to draw.
    pub(in crate::window_view) fn compare_menu_view(&self) -> Option<Entity<ContextMenu>> {
        let screen = self.compare.screen.as_ref()?;
        let menu = match (&screen.file, &screen.stream) {
            (Some(file), _) => file.menu.as_ref(),
            (None, Some(stream)) => stream.menu.as_ref(),
            (None, None) => None,
        };
        menu.map(|(menu, _)| menu.clone())
    }
}
