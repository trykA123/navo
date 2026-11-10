//! The Setup window: a GPUI entity that draws the wizard and runs the core's outputs.
//!
//! Effects go to the worker thread; replies come back on a channel. Close waits for the worker.
//! Keys and the Folder field live in `window_input.rs`.

use std::sync::mpsc::Sender;
use std::time::Duration;

use futures::StreamExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, ClipboardItem, Context, Entity, FocusHandle,
    InteractiveElement, IntoElement, PathPromptOptions, Render, Subscription, Task,
    TitlebarOptions, Window, WindowAppearance, WindowBounds, WindowOptions, px, size,
};
use navo_ui::primitives::TextFieldState;
use navo_ui::setup::keys::footer;
use navo_ui::setup::{OnAction, SetupAction, Strings, render_frame};
use navo_ui::tokens::{self, Mode};
use navo_ui::{ParentElement, Styled, div};

use crate::flow::{Event, Page};
use crate::syntax::Candidate;
use crate::window_core::{Out, Shell};
use crate::window_worker::{Job, Link, Reply};

/// The pause after the last keystroke before the folder is checked.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// The wizard window's state: the pure core, its texts and the worker's job queue.
pub struct SetupWindow {
    pub(crate) shell: Shell,
    pub(crate) strings: Strings,
    jobs: Sender<Job>,
    /// The pending folder check; replacing or dropping it cancels the earlier timer.
    debounce: Option<Task<()>>,
    /// The editable Folder field; its edits feed the debounced check.
    pub(crate) field: Entity<TextFieldState>,
    /// The window root's focus: Enter there runs the default button.
    pub(crate) focus: FocusHandle,
    /// The footer's primary button; focus starts here so it shows and Space presses it.
    pub(crate) primary: FocusHandle,
    /// The page drawn last; a change moves focus to the new page's default.
    page: Page,
    handle: AnyWindowHandle,
    _replies: Task<()>,
    _appearance: Subscription,
    _field: Subscription,
}

impl SetupWindow {
    /// Builds the window state, listens for worker replies and guards the title-bar Close.
    pub fn new(shell: Shell, link: Link, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Link { jobs, mut replies } = link;
        let listen = cx.spawn_in(window, async move |this, cx| {
            // Stops when the worker's channel ends or the window is gone.
            while let Some(reply) = replies.next().await
                && this.update(cx, |view, cx| view.reply(reply, cx)).is_ok()
            {}
            let _ = this.update(cx, |view, cx| {
                let outs = view.shell.worker_lost();
                view.run(outs, cx);
            });
        });
        let this = cx.entity().downgrade();
        // The flow decides: Close is ignored while Installing, and the process quits after cleanup.
        window.on_window_should_close(cx, move |_, cx| {
            let _ = this.update(cx, |view, cx| view.act(SetupAction::Close, cx));
            false
        });
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            tokens::apply(mode_of(window.appearance()), cx);
            cx.notify();
        });
        let strings = Strings::load();
        let (field, field_changes) = Self::folder_field(&shell, &strings, window, cx);
        let focus = cx.focus_handle();
        let primary = cx.focus_handle();
        let page = shell.page();
        let drawn = footer(&shell.view(), &strings)
            .iter()
            .any(|button| button.primary);
        window.focus(if drawn { &primary } else { &focus }, cx);
        Self {
            shell,
            strings,
            jobs,
            debounce: None,
            field,
            focus,
            primary,
            page,
            handle: window.window_handle(),
            _replies: listen,
            _appearance: appearance,
            _field: field_changes,
        }
    }

    /// Returns the pure core, for tests.
    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    /// Returns the window handle, for tests that need to inspect or move focus.
    pub(crate) fn window_handle(&self) -> AnyWindowHandle {
        self.handle
    }

    /// Runs one page or footer action.
    pub fn act(&mut self, action: SetupAction, cx: &mut Context<Self>) {
        let outs = self.shell.act(action);
        self.run(outs, cx);
    }

    /// Feeds one flow event, such as a typed folder, through the core.
    pub fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        let outs = self.shell.handle(event);
        self.run(outs, cx);
    }

    /// Applies one worker reply.
    fn reply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        let outs = self.shell.reply(reply);
        self.run(outs, cx);
    }

    /// Executes the core's outputs in order, then redraws.
    fn run(&mut self, outs: Vec<Out>, cx: &mut Context<Self>) {
        for out in outs {
            match out {
                Out::Send(job) => self.send(job, cx),
                Out::CheckLater(candidate) => self.check_later(candidate, cx),
                Out::CancelCheck => self.debounce = None,
                Out::Browse => self.browse(cx),
                Out::OpenFolder(path) => cx.open_with_system(&path),
                Out::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                Out::Quit => cx.quit(),
            }
        }
        let page = self.shell.page();
        if page != self.page {
            self.page = page;
            self.focus_page(cx);
        }
        cx.notify();
    }

    /// Checks `candidate` after the typing pause; replacing the task cancels the earlier timer.
    fn check_later(&mut self, candidate: Candidate, cx: &mut Context<Self>) {
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| view.send(Job::Check(candidate), cx));
        }));
    }

    /// Moves focus to the new page's default: the footer primary, or the root that owns the keys.
    fn focus_page(&mut self, cx: &mut Context<Self>) {
        let drawn = footer(&self.shell.view(), &self.strings)
            .iter()
            .any(|button| button.primary);
        let focus = if drawn {
            self.primary.clone()
        } else {
            self.focus.clone()
        };
        let handle = self.handle;
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, cx| window.focus(&focus, cx));
        });
    }

    /// Queues one job; a worker that is gone turns into a failed run or a quit.
    fn send(&mut self, job: Job, cx: &mut Context<Self>) {
        if matches!(job, Job::Install(_) | Job::Close) {
            self.debounce = None;
        }
        if self.jobs.send(job).is_err() {
            let outs = self.shell.worker_lost();
            self.run(outs, cx);
        }
    }

    /// Opens the native folder picker; the picked parent becomes `<parent>\Navo` in the flow.
    fn browse(&mut self, cx: &mut Context<Self>) {
        let handle = self.handle;
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(parent) = paths.into_iter().next() else {
                return;
            };
            let _ = cx.update_window(handle, |_, window, cx| {
                this.update(cx, |view, cx| view.picked(parent, window, cx))
            });
        })
        .detach();
    }

    /// Builds the action sink the pages call.
    fn sink(cx: &Context<Self>) -> OnAction {
        let this = cx.entity().downgrade();
        std::rc::Rc::new(move |action, _, cx| {
            let _ = this.update(cx, |view, cx| view.act(action, cx));
        })
    }
}

impl Render for SetupWindow {
    /// Draws the rail, the current page and the footer; the root takes the window's keys.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.shell.view();
        let frame = render_frame(
            &view,
            &self.strings,
            Self::sink(cx),
            (Some(self.field.clone()), Some(self.primary.clone())),
        );
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .child(frame)
    }
}

/// Maps the system appearance to the token mode; the window follows Windows (IW21).
pub fn mode_of(appearance: WindowAppearance) -> Mode {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
    }
}

/// The fixed window size in logical pixels (IW21).
const WIDTH: f32 = 640.;
const HEIGHT: f32 = 480.;

/// Opens the fixed, centred window and runs the app until the core quits; errors name the step.
pub fn launch(shell: Shell, link: Link) -> Result<(), String> {
    let failed = std::rc::Rc::new(std::cell::RefCell::new(None));
    let failure = failed.clone();
    navo_ui::application().run(move |cx| {
        // A font load error leaves the system fallback font; the wizard still works.
        let _ = navo_ui::init(mode_of(cx.window_appearance()), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(WIDTH), px(HEIGHT)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some(navo_l10n::tr("setup-window-title").into()),
                ..TitlebarOptions::default()
            }),
            is_resizable: false,
            app_id: Some("navo-setup".into()),
            ..WindowOptions::default()
        };
        let opened = navo_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| SetupWindow::new(shell, link, window, cx))
        });
        if let Err(error) = opened {
            *failure.borrow_mut() = Some(format!("could not open the Setup window: {error:#}"));
            cx.quit();
        }
    });
    failed.take().map_or(Ok(()), Err)
}
