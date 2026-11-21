//! The toast layer (NC66/NC68): up to three solid-surface toasts, each expiring on its own
//! timer, with a single-step Undo. Entrances go through the U12b reduced-motion gate.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{AnimationExt, Global, ReadGlobal, Task, WeakEntity};
use sidewise_kit::prelude::*;

use crate::motion;
use crate::theme::SidewiseTheme;
use crate::theme_data::palettes::ModeTokens;

/// How many toasts may be on screen at once; a fourth retires the oldest.
const CAP: usize = 3;
/// How long a toast lingers before its own timer expires it (`setTimeout` in the prototype).
const LINGER: Duration = Duration::from_millis(4200);

/// A toast's Undo action, run on the UI thread. It is single-step because pressing it dismisses
/// the toast, so the same action can never run twice.
type Undo = Rc<dyn Fn(&mut App)>;

/// One toast: its message, its optional Undo and the timer that expires it.
struct Toast {
    serial: u64,
    message: SharedString,
    undo: Option<Undo>,
    /// Held so dropping the host cancels the pending expiry rather than leaving it parked.
    _timer: Task<()>,
}

/// The toast queue: at most `CAP` toasts, oldest first.
pub struct ToastHost {
    toasts: Vec<Toast>,
    next_serial: u64,
}

impl ToastHost {
    /// An empty host.
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_serial: 0,
        }
    }

    /// How many toasts are on screen.
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// Whether nothing is on screen.
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    /// The texts on screen, oldest first; tests read them.
    #[cfg(test)]
    pub fn messages(&self) -> Vec<SharedString> {
        self.toasts
            .iter()
            .map(|toast| toast.message.clone())
            .collect()
    }

    /// Shows a toast that expires on its own.
    pub fn push(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.add(message, None, cx);
    }

    /// Shows a toast with a single-step Undo: pressing it runs `undo` once and dismisses.
    pub fn push_undo(
        &mut self,
        message: impl Into<SharedString>,
        undo: impl Fn(&mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.add(message, Some(Rc::new(undo)), cx);
    }

    /// Runs the newest Undo and dismisses its toast; toasts without one are left alone.
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.toasts.iter().rposition(|toast| toast.undo.is_some()) else {
            return;
        };
        let toast = self.toasts.remove(at);
        if let Some(undo) = toast.undo {
            undo(cx);
        }
        cx.notify();
    }

    /// Adds one toast, retiring the oldest past the cap, and arms its expiry.
    fn add(
        &mut self,
        message: impl Into<SharedString>,
        undo: Option<Undo>,
        cx: &mut Context<Self>,
    ) {
        let serial = self.next_serial;
        self.next_serial += 1;
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LINGER).await;
            let _ = this.update(cx, |host, cx| host.expire(serial, cx));
        });
        self.toasts.push(Toast {
            serial,
            message: message.into(),
            undo,
            _timer: timer,
        });
        if self.toasts.len() > CAP {
            self.toasts.remove(0);
        }
        cx.notify();
    }

    /// Drops one toast when its timer fires; a serial already gone is ignored.
    fn expire(&mut self, serial: u64, cx: &mut Context<Self>) {
        if let Some(at) = self.toasts.iter().position(|toast| toast.serial == serial) {
            self.toasts.remove(at);
            cx.notify();
        }
    }
}

/// The app-wide handle to the shell's toast host (NC140), so any view can toast.
struct ToastSink(WeakEntity<ToastHost>);

impl Global for ToastSink {}

/// Registers `host` as the app's toast sink; the shell calls this once it has made its host.
pub fn install_sink(host: &Entity<ToastHost>, cx: &mut App) {
    cx.set_global(ToastSink(host.downgrade()));
}

/// Toasts `text` on the installed host. Returns `false` when no host is up (tests, closed window).
pub fn notify(text: impl Into<SharedString>, cx: &mut App) -> bool {
    let text = text.into();
    with_host(cx, |host, cx| host.push(text, cx))
}

/// Toasts `text` with a single-step Undo. Returns `false` when no host is up.
pub fn notify_undo(
    text: impl Into<SharedString>,
    undo: impl Fn(&mut App) + 'static,
    cx: &mut App,
) -> bool {
    let text = text.into();
    with_host(cx, |host, cx| host.push_undo(text, undo, cx))
}

/// Runs `f` on the installed host if it is still alive; `false` when there is none.
fn with_host(cx: &mut App, f: impl FnOnce(&mut ToastHost, &mut Context<ToastHost>)) -> bool {
    let Some(host) = cx
        .try_global::<ToastSink>()
        .and_then(|sink| sink.0.upgrade())
    else {
        return false;
    };
    host.update(cx, f);
    true
}

impl Render for ToastHost {
    /// The stack on the solid `--toast-bg` surface with a `--sep-strong` hairline: no backdrop
    /// blur, which gpui draws with artifacts.
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = *SidewiseTheme::global(cx);
        let m = if side.mode.is_dark() {
            side.spec.dark
        } else {
            side.spec.light
        };
        div().flex().flex_col().gap_2().children(
            self.toasts
                .iter()
                .map(|toast| toast_row(toast, &m, side.motion.menu_ms, cx)),
        )
    }
}

/// One toast: its solid surface and hairline, its Undo button, and its gated fade-in.
fn toast_row(
    toast: &Toast,
    m: &ModeTokens,
    enter_ms: u64,
    cx: &mut Context<ToastHost>,
) -> AnyElement {
    let [bg, fg, acc] = m.toast;
    let serial = toast.serial;
    let id = SharedString::from(format!("toast-{serial}"));
    let mut row = div()
        .flex()
        .items_center()
        .gap_3()
        .px_4()
        .py_2()
        .rounded(px(14.))
        .border_1()
        .border_color(m.sep_strong)
        .bg(bg)
        .text_color(fg)
        .child(toast.message.clone());
    if toast.undo.is_some() {
        row = row.child(
            div()
                .id(SharedString::from(format!("toast-undo-{serial}")))
                .text_color(acc)
                .child("Undo")
                .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
        );
    }
    match motion::anim(id.clone(), enter_ms, cx) {
        Some(enter) => row
            .with_animation(id, enter, |el, delta| el.opacity(delta))
            .into_any_element(),
        None => row.into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt;
    use navo_compare_profile::appearance::PaletteId;
    use sidewise_kit::tokens::Mode;

    use super::*;
    use crate::test_window::open;
    use crate::theme::apply_sidewise;

    /// A host with the Sidewise tokens registered, so the render path has its colours.
    fn host(cx: &mut TestAppContext) -> Entity<ToastHost> {
        cx.update(|cx| {
            gpui_kit::init(cx);
            apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        });
        cx.new(|_| ToastHost::new())
    }

    /// Four toasts leave three on screen, and Undo fires its callback exactly once.
    #[gpui_kit::test]
    fn four_toasts_cap_at_three_and_undo_fires_once(cx: &mut TestAppContext) {
        let host = host(cx);
        let fired = Rc::new(Cell::new(0));
        cx.update(|cx| {
            for at in 0..4 {
                let fired = Rc::clone(&fired);
                host.update(cx, |host, cx| {
                    host.push_undo(
                        format!("Deleted {at}"),
                        move |_| fired.set(fired.get() + 1),
                        cx,
                    );
                });
            }
        });
        cx.update(|cx| {
            assert_eq!(
                host.read(cx).len(),
                CAP,
                "the fourth toast must retire the oldest"
            );
            host.update(cx, |host, cx| host.undo(cx));
            assert_eq!(fired.get(), 1, "Undo must fire once");
            assert_eq!(host.read(cx).len(), CAP - 1, "Undo dismisses its toast");
            host.update(cx, |host, cx| host.undo(cx));
            assert_eq!(fired.get(), 2, "the next Undo is the next toast's");
        });
    }

    /// A toast expires on its own timer, and only that toast goes.
    #[gpui_kit::test]
    fn a_toast_expires_on_its_own_timer(cx: &mut TestAppContext) {
        let host = host(cx);
        cx.update(|cx| {
            host.update(cx, |host, cx| host.push("first", cx));
            host.update(cx, |host, cx| host.push("second", cx));
        });
        cx.executor().advance_clock(LINGER);
        cx.run_until_parked();
        cx.update(|cx| assert!(host.read(cx).is_empty(), "both toasts must expire"));
    }

    /// Dropping the host leaves no parked timer holding it alive, and parking never hangs.
    #[gpui_kit::test]
    fn dropping_the_host_cancels_the_timers(cx: &mut TestAppContext) {
        let host = host(cx);
        let weak = host.downgrade();
        cx.update(|cx| host.update(cx, |host, cx| host.push("gone", cx)));
        drop(host);
        cx.run_until_parked();
        cx.executor().advance_clock(LINGER);
        cx.run_until_parked();
        cx.update(|_| {
            assert!(
                weak.upgrade().is_none(),
                "a pending expiry must not keep the host alive"
            );
        });
    }

    /// The entrance uses the menu token, and reduced motion renders the toast statically.
    #[gpui_kit::test]
    fn the_entrance_honours_reduced_motion(cx: &mut TestAppContext) {
        let host = host(cx);
        cx.update(|cx| {
            assert_eq!(
                motion::anim("toast", SidewiseTheme::global(cx).motion.menu_ms, cx)
                    .expect("motion is allowed")
                    .duration,
                Duration::from_millis(180),
                "the entrance is the 180ms menu token"
            );
            host.update(cx, |host, cx| host.push("hello", cx));
            cx.set_reduce_motion(true);
            assert!(
                motion::anim("toast", SidewiseTheme::global(cx).motion.menu_ms, cx).is_none(),
                "reduced motion must stop the entrance"
            );
        });
    }

    /// Smoke render: the layer draws its solid surface, hairline and Undo button, both with the
    /// animated entrance and with reduced motion's static one. Asserts only that drawing works.
    #[gpui_kit::test]
    fn the_layer_draws_with_and_without_the_entrance(cx: &mut TestAppContext) {
        // The tokens must exist before the window opens: opening draws the first frame.
        cx.update(|cx| {
            gpui_kit::init(cx);
            apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        });
        let (window, host) = open(cx, |_, _| ToastHost::new());
        cx.update_window(window, |_, window, cx| {
            host.update(cx, |host, cx| {
                host.push("plain", cx);
                host.push_undo("with undo", |_| {}, cx);
            });
            window.render_frame(cx);
            cx.set_reduce_motion(true);
            window.render_frame(cx);
        })
        .expect("window");
        cx.update(|cx| assert_eq!(host.read(cx).len(), 2, "drawing must not consume toasts"));
    }

    /// A notification through the installed sink lands on that host.
    #[gpui_kit::test]
    fn notify_reaches_the_installed_host(cx: &mut TestAppContext) {
        let host = host(cx);
        cx.update(|cx| {
            install_sink(&host, cx);
            assert!(notify("hi", cx), "an installed host takes the toast");
            assert!(notify_undo("gone", |_| {}, cx), "and the undo toast");
        });
        cx.update(|cx| assert_eq!(host.read(cx).len(), 2));
    }

    /// With no host installed, or a dropped one, notify reports `false`.
    #[gpui_kit::test]
    fn notify_without_a_host_is_false(cx: &mut TestAppContext) {
        cx.update(|cx| assert!(!notify("nobody", cx), "no sink means no toast"));
        let host = host(cx);
        cx.update(|cx| install_sink(&host, cx));
        drop(host);
        cx.update(|cx| assert!(!notify("dropped", cx), "a dropped host means no toast"));
    }
}
