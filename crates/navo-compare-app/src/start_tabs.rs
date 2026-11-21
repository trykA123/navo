//! Start screen v2 parts: the Folder/File tab pill, the `.gitignore` rule chip and the hero art.
//! The state lives in one `StartTabs` entity that `StartScreen` owns; `submit` reads its choice.
//! The Tab order of the whole screen lives in `start_focus`.

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{
    FocusHandle, PaintQuad, Role, Stateful, Toggled, canvas, linear_color_stop, linear_gradient,
};
use sidewise_kit::prelude::*;
use sidewise_kit::tokens;

use crate::start::Side;
use crate::start_focus::Stop;
use crate::theme::SidewiseTheme;

/// What the two fields hold: two folders or two files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartMode {
    #[default]
    Folder,
    File,
}

impl StartMode {
    /// The visible label of one field; the file mode names its kind, the folder mode keeps the bare side.
    pub fn field_label(self, side: Side) -> &'static str {
        match (self, side) {
            (Self::Folder, Side::Left) => "Left",
            (Self::Folder, Side::Right) => "Right",
            (Self::File, Side::Left) => "Left file",
            (Self::File, Side::Right) => "Right file",
        }
    }

    /// The placeholder of both fields: it names what the mode compares.
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Folder => "Folder path",
            Self::File => "File path",
        }
    }
}

/// What the user chose on the start screen besides the two sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartChoice {
    pub mode: StartMode,
    pub respect_gitignore: bool,
}

impl Default for StartChoice {
    /// Folders, with the `.gitignore` rule on as in the prototype.
    fn default() -> Self {
        Self {
            mode: StartMode::Folder,
            respect_gitignore: true,
        }
    }
}

/// The state entity behind the tab pill and the rule chip; it also owns their focus handles.
pub struct StartTabs {
    choice: StartChoice,
    folder: FocusHandle,
    file: FocusHandle,
    pub(crate) compare: FocusHandle,
    pub(crate) rule: FocusHandle,
}

impl StartTabs {
    /// Makes the default choice (folders, rule on) with one focus handle per non-field stop.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            choice: StartChoice::default(),
            folder: cx.focus_handle(),
            file: cx.focus_handle(),
            compare: cx.focus_handle(),
            rule: cx.focus_handle(),
        }
    }

    /// The current choice.
    pub fn choice(&self) -> StartChoice {
        self.choice
    }

    /// Switches the tab and asks for a redraw.
    pub fn set_mode(&mut self, mode: StartMode, cx: &mut Context<Self>) {
        self.choice.mode = mode;
        cx.notify();
    }

    /// Flips the `.gitignore` rule and asks for a redraw.
    pub fn toggle_gitignore(&mut self, cx: &mut Context<Self>) {
        self.choice.respect_gitignore = !self.choice.respect_gitignore;
        cx.notify();
    }

    /// The focus handle of a mode's tab.
    fn tab_handle(&self, mode: StartMode) -> &FocusHandle {
        match mode {
            StartMode::Folder => &self.folder,
            StartMode::File => &self.file,
        }
    }

    /// The focus handle of a stop that is not a text field; `None` for `Left`, `Right` and the update buttons.
    pub(crate) fn handle(&self, stop: Stop) -> Option<&FocusHandle> {
        match stop {
            Stop::Folder => Some(self.tab_handle(StartMode::Folder)),
            Stop::File => Some(self.tab_handle(StartMode::File)),
            Stop::Compare => Some(&self.compare),
            Stop::Rule => Some(&self.rule),
            Stop::Left | Stop::Right | Stop::UpdateFirst | Stop::UpdateSecond => None,
        }
    }
}

/// One toggle cell: a focusable, clickable pill; `pressed` holds its (fill, text) look when on.
/// A click by key (Enter or Space on the focused cell) reaches the caller's `on_click` too.
fn cell(
    id: &'static str,
    label: &'static str,
    pressed: Option<(Hsla, Hsla)>,
    cx: &App,
) -> Stateful<Div> {
    let t = tokens::current(cx);
    let on = pressed.is_some();
    let (bg, fg) = pressed.unwrap_or((gpui_kit::transparent_black(), t.colors.muted));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .aria_toggled(if on { Toggled::True } else { Toggled::False })
        .cursor_pointer()
        .flex_none()
        .self_start()
        .px(t.spacing.lg)
        .py(t.spacing.xs)
        .border_1()
        .border_color(gpui_kit::transparent_black())
        .focus_visible(|s| s.border_color(t.colors.accent))
        .bg(bg)
        .text_color(fg)
        .text_size(t.typography.small)
        .when(on, |c| c.font_weight(FontWeight::SEMIBOLD))
        .child(label)
}

/// The segmented Folder/File pill; the Git tab is hidden in the alpha.
pub fn mode_tabs(state: &Entity<StartTabs>, mode: StartMode, cx: &App) -> impl IntoElement {
    let t = tokens::current(cx);
    let tab = |id, label, which: StartMode| {
        let look = (mode == which).then_some((t.colors.surface, t.colors.text));
        let focus = state.read(cx).tab_handle(which).clone();
        let state = state.clone();
        cell(id, label, look, cx)
            .on_click(move |_, _, cx| state.update(cx, |s, cx| s.set_mode(which, cx)))
            .track_focus(&focus)
            .rounded(t.radii.md)
            .test_support()
    };
    div()
        .flex()
        .flex_none()
        .self_start()
        .gap(t.spacing.xxs)
        .p(t.spacing.xxs)
        .rounded(t.radii.lg)
        .bg(t.colors.zebra)
        .child(tab("start-tab-folder", "Folder", StartMode::Folder))
        .child(tab("start-tab-file", "File", StartMode::File))
}

/// The "Respect .gitignore" rule chip; it is pressed while the rule is on.
pub fn rule_chip(state: &Entity<StartTabs>, on: bool, cx: &App) -> impl IntoElement {
    let t = tokens::current(cx);
    let look = on.then_some((t.colors.selection, t.colors.accent_text));
    let focus = state.read(cx).rule.clone();
    let state = state.clone();
    cell("start-rule-gitignore", "Respect .gitignore", look, cx)
        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.toggle_gitignore(cx)))
        .track_focus(&focus)
        .rounded(t.radii.pill)
        .test_support()
}

/// The art colours: the palette's four art tokens plus the ink; navo's accent stands in until the
/// Sidewise theme is installed.
fn art_colors(cx: &App) -> ([Hsla; 4], Hsla) {
    let t = tokens::current(cx);
    let Some(side) = cx.try_global::<SidewiseTheme>() else {
        return ([t.colors.accent; 4], gpui_kit::hsla(0., 0., 0.98, 0.92));
    };
    let m = if side.mode.is_dark() {
        side.spec.dark
    } else {
        side.spec.light
    };
    (m.art, gpui_kit::hsla(m.art[0].h, 0.3, 0.96, 0.92))
}

/// Paints one pane: a gradient card with a title bar and a shorter text bar in the ink.
fn paint_pane(bounds: Bounds<Pixels>, [from, to]: [Hsla; 2], ink: Hsla, window: &mut Window) {
    let gradient = linear_gradient(135., linear_color_stop(from, 0.), linear_color_stop(to, 1.));
    let card = PaintQuad {
        corner_radii: px(10.).into(),
        ..gpui_kit::fill(bounds, gradient)
    };
    window.paint_quad(card);
    for (y, w, h, alpha) in [(0.55, 0.5, 10., 1.), (0.72, 0.34, 6., 0.65)] {
        let origin = bounds.origin + point(px(16.), bounds.size.height * y);
        let rect = Bounds {
            origin,
            size: size(bounds.size.width * w, px(h)),
        };
        let color = Hsla {
            a: ink.a * alpha,
            ..ink
        };
        window.paint_quad(PaintQuad {
            corner_radii: px(h / 2.).into(),
            ..gpui_kit::fill(rect, color)
        });
    }
}

/// The empty-state art: two panes side by side, painted from the palette's art tokens.
pub fn hero(cx: &App) -> impl IntoElement {
    let ([a1, a2, b1, b2], ink) = art_colors(cx);
    let art = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let half = size((bounds.size.width - px(12.)) / 2., bounds.size.height);
            for (i, tones) in [[a1, a2], [b1, b2]].into_iter().enumerate() {
                let origin = bounds.origin + point((half.width + px(12.)) * i as f32, px(0.));
                paint_pane(Bounds { origin, size: half }, tones, ink, window);
            }
        },
    )
    .size_full();
    div()
        .id("start-hero")
        .test_support()
        .w_full()
        .max_w(px(420.))
        .h(px(120.))
        .mt(tokens::current(cx).spacing.xxl)
        .child(art)
}

#[cfg(test)]
mod start_v2_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui_kit::base::test_support::{find, texts};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AnyWindowHandle, ElementId, Entity, TestAppContext, VisualTestContext};
    use navo_compare_profile::recent::StartRows;
    use sidewise_kit::tokens::Mode;

    use super::*;
    use crate::start::StartScreen;
    use crate::test_window::open;
    use crate::theme::apply_sidewise;
    use crate::theme_data::palettes::PALETTES;
    use navo_compare_profile::appearance::PaletteId;

    type Handle = Rc<RefCell<Option<Entity<StartTabs>>>>;

    /// Opens a start screen and captures its `StartTabs` entity when the first frame makes it.
    fn open_start(cx: &mut TestAppContext) -> (AnyWindowHandle, Handle) {
        let handle: Handle = Rc::default();
        let slot = handle.clone();
        cx.update(|cx| {
            cx.observe_new(move |_: &mut StartTabs, _, cx| *slot.borrow_mut() = Some(cx.entity()))
                .detach();
        });
        let cwd = std::env::current_dir().unwrap_or_default();
        let (window, _): (_, Entity<StartScreen>) = open(cx, move |w, cx| {
            StartScreen::new(StartRows::default(), cwd, w, cx)
        });
        cx.update_window(window, |_, w, cx| w.render_frame(cx))
            .expect("window");
        (window, handle)
    }

    /// Clicks the element `id` at its centre and draws the next frame.
    fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
        let at = cx
            .update_window(window, |_, w, _| w.find(id).bounds().center())
            .expect("window");
        VisualTestContext::from_window(window, cx).simulate_click(at, Default::default());
        cx.run_until_parked();
        cx.update_window(window, |_, w, cx| w.render_frame(cx))
            .expect("window");
    }

    /// Reads the captured choice.
    fn choice(cx: &mut TestAppContext, handle: &Handle) -> StartChoice {
        let tabs = handle.borrow().clone().expect("tabs entity");
        tabs.read_with(cx, |t, _| t.choice())
    }

    /// Counts visible text nodes equal to `text`.
    fn count(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) -> usize {
        cx.update_window(window, |_, w, _| {
            texts(w)
                .into_iter()
                .filter(|e| e.visible() && e.text() == text)
                .count()
        })
        .expect("window")
    }

    /// Clicking the File tab switches the mode and relabels both fields; Folder switches back.
    #[gpui_kit::test]
    fn start_v2_tabs_switch_mode_and_field_labels(cx: &mut TestAppContext) {
        let (window, handle) = open_start(cx);
        assert_eq!(choice(cx, &handle).mode, StartMode::Folder);
        assert_eq!(
            (count(cx, window, "Left"), count(cx, window, "Left file")),
            (1, 0)
        );
        click(cx, window, "start-tab-file");
        assert_eq!(choice(cx, &handle).mode, StartMode::File);
        assert_eq!(
            (
                count(cx, window, "Left file"),
                count(cx, window, "Right file")
            ),
            (1, 1)
        );
        assert_eq!(count(cx, window, "Left"), 0);
        click(cx, window, "start-tab-folder");
        assert_eq!(choice(cx, &handle).mode, StartMode::Folder);
        assert_eq!(count(cx, window, "Right"), 1);
    }

    /// The `.gitignore` chip starts on and flips with each click, leaving the mode alone.
    #[gpui_kit::test]
    fn start_v2_gitignore_chip_toggles(cx: &mut TestAppContext) {
        let (window, handle) = open_start(cx);
        assert!(choice(cx, &handle).respect_gitignore);
        click(cx, window, "start-rule-gitignore");
        assert_eq!(
            choice(cx, &handle),
            StartChoice {
                respect_gitignore: false,
                ..StartChoice::default()
            }
        );
        click(cx, window, "start-rule-gitignore");
        assert!(choice(cx, &handle).respect_gitignore);
    }

    /// With no stored rows the hero card is drawn, inside the window and with a real size.
    #[gpui_kit::test]
    fn start_v2_hero_shows_without_rows(cx: &mut TestAppContext) {
        let (window, _) = open_start(cx);
        let hero = cx
            .update_window(window, |_, w, _| {
                find(w, &[], &ElementId::Name("start-hero".into())).map(|s| s.bounds())
            })
            .expect("window");
        assert!(hero.is_some(), "the hero is drawn when there are no rows");
        let b = hero.unwrap_or_default();
        assert!(
            b.size.width > px(100.) && b.size.height > px(60.),
            "hero too small: {b:?}"
        );
    }

    /// The hero reads the palette's art tokens: Ocean dark gives Ocean's four art colours.
    #[gpui_kit::test]
    fn start_v2_hero_colors_follow_the_palette(cx: &mut TestAppContext) {
        let _ = open_start(cx);
        let (art, ink) = cx.update(|cx| {
            apply_sidewise(PaletteId::Ocean, Mode::Dark, cx);
            art_colors(cx)
        });
        let want = PALETTES[PaletteId::Ocean as usize].1.dark;
        assert_eq!(art, want.art);
        assert!(
            ink.l > 0.9 && ink.a > 0.9,
            "the ink is light on every palette"
        );
    }
}
