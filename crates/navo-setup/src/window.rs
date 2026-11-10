//! The Setup window: a GPUI entity that draws the wizard and runs the core's outputs.
//!
//! Effects go to the worker thread; replies come back on a channel. Close waits for the worker.

use std::sync::mpsc::Sender;
use std::time::Duration;

use futures::StreamExt;
use gpui_kit::{
    AppContext, Bounds, ClipboardItem, Context, IntoElement, PathPromptOptions, Render,
    Subscription, Task, TitlebarOptions, Window, WindowAppearance, WindowBounds, WindowOptions, px,
    size,
};
use navo_ui::setup::{OnAction, SetupAction, Strings, render_page};
use navo_ui::tokens::{self, Mode};
use navo_ui::{ParentElement, Styled, div};

use crate::flow::Event;
use crate::syntax::Candidate;
use crate::window_core::{Out, Shell};
use crate::window_worker::{Job, Link, Reply};

/// The pause after the last keystroke before the folder is checked.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// The wizard window's state: the pure core, its texts and the worker's job queue.
pub struct SetupWindow {
    shell: Shell,
    strings: Strings,
    jobs: Sender<Job>,
    /// The pending folder check; replacing or dropping it cancels the earlier timer.
    debounce: Option<Task<()>>,
    _replies: Task<()>,
    _appearance: Subscription,
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
        Self {
            shell,
            strings: Strings::load(),
            jobs,
            debounce: None,
            _replies: listen,
            _appearance: appearance,
        }
    }

    /// Returns the pure core, for tests.
    pub fn shell(&self) -> &Shell {
        &self.shell
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
        cx.notify();
    }

    /// Checks `candidate` after the typing pause; replacing the task cancels the earlier timer.
    fn check_later(&mut self, candidate: Candidate, cx: &mut Context<Self>) {
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| view.send(Job::Check(candidate), cx));
        }));
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
            if let Some(parent) = paths.into_iter().next() {
                let _ = this.update(cx, |view, cx| view.handle(Event::BrowsePicked(parent), cx));
            }
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
    /// Draws the current page on the theme surface.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        let page = render_page(&self.shell.view(), &self.strings, Self::sink(cx));
        div()
            .size_full()
            .p(t.spacing.xl)
            .bg(t.colors.surface)
            .child(page)
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
