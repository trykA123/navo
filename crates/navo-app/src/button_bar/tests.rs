use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedReceiver;
use navo_icons::spec::{Shape, Tone};
use navo_store::StoreDirs;
use navo_ui::window_view::{
    BarIcon, ButtonAction, ButtonBarNews, ButtonBarReceiver, ProgramId, UserCommandNews,
    UserInvocation,
};

use super::hooks;
use crate::user_commands;

const NEWS_WAIT: Duration = Duration::from_secs(10);

fn temp_dirs() -> (tempfile::TempDir, StoreDirs) {
    let root = tempfile::tempdir().expect("a temporary folder");
    let dirs = StoreDirs {
        config: root.path().join("config"),
        data: root.path().join("data"),
    };
    dirs.ensure().expect("the settings folders");
    (root, dirs)
}

/// Wakes the waiting test thread when a channel has an item.
struct Unpark(std::thread::Thread);

impl futures::task::ArcWake for Unpark {
    fn wake_by_ref(this: &Arc<Self>) {
        this.0.unpark();
    }
}

fn next_user_until(
    receiver: &mut UnboundedReceiver<UserCommandNews>,
    deadline: Instant,
) -> Option<UserCommandNews> {
    use futures::StreamExt as _;
    let waker = futures::task::waker(Arc::new(Unpark(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    loop {
        if let std::task::Poll::Ready(news) = receiver.poll_next_unpin(&mut context) {
            return news;
        }
        let left = deadline.checked_duration_since(Instant::now())?;
        std::thread::park_timeout(left);
    }
}

fn next_bar_until(receiver: &mut ButtonBarReceiver, deadline: Instant) -> Option<ButtonBarNews> {
    let waker = futures::task::waker(Arc::new(Unpark(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut fut = Box::pin(receiver.recv());
    loop {
        if let std::task::Poll::Ready(news) = fut.as_mut().poll(&mut context) {
            return news;
        }
        let left = deadline.checked_duration_since(Instant::now())?;
        std::thread::park_timeout(left);
    }
}

fn write_until(
    path: &Path,
    text: &str,
    receiver: &mut ButtonBarReceiver,
    wanted: impl Fn(&ButtonBarNews) -> bool,
) -> ButtonBarNews {
    let deadline = Instant::now() + NEWS_WAIT;
    loop {
        std::fs::write(path, text).expect("write buttonbar.toml");
        let round = Instant::now() + Duration::from_millis(500);
        while let Some(news) = next_bar_until(receiver, round) {
            if wanted(&news) {
                return news;
            }
        }
        assert!(Instant::now() < deadline, "no matching news arrived");
    }
}

fn first_program_id(news: &ButtonBarNews) -> Option<ProgramId> {
    match news {
        ButtonBarNews::Applied { items, .. } => items.iter().find_map(|item| match &item.action {
            ButtonAction::Program { id, .. } => Some(*id),
            _ => None,
        }),
        ButtonBarNews::Invalid(_) => None,
    }
}

#[test]
fn button_bar_missing_file_applies_empty() {
    let (_root, dirs) = temp_dirs();
    let (_uc_hooks, _uc_watch, runner) = user_commands::hooks(Some(&dirs));

    let (no_dirs_hooks, no_dirs_watch) = hooks(None, runner.clone());
    assert!(no_dirs_watch.is_none());
    assert!(no_dirs_hooks.file.is_none());
    assert!(no_dirs_hooks.news.is_none());
    assert_eq!((no_dirs_hooks.undo)().unwrap_err(), "no settings folder");

    let (mut bar_hooks, _watch) = hooks(Some(&dirs), runner);
    assert!(bar_hooks.file.is_some());
    assert_eq!((bar_hooks.undo)().unwrap_err(), "not ready");
    let mut news = bar_hooks.news.take().expect("news receiver");
    assert_eq!(
        news.try_recv(),
        Some(ButtonBarNews::Applied {
            items: Vec::new(),
            warnings: Vec::new(),
        })
    );
}

#[test]
fn button_bar_invalid_file_keeps_last_and_reports_line() {
    let (_root, dirs) = temp_dirs();
    let path = dirs.button_bar_file();
    std::fs::write(
        &path,
        "[[item]]\nname = \"Run\"\nprogram = \"echo\"\nargs = \"{file}\"\n",
    )
    .expect("write initial buttonbar.toml");

    let (mut uc_hooks, _uc_watch, runner) = user_commands::hooks(Some(&dirs));
    let mut uc_news = uc_hooks.news.take().expect("user command news");
    let _ = uc_news.try_recv();

    let (mut bar_hooks, _watch) = hooks(Some(&dirs), runner);
    let mut bar_news = bar_hooks.news.take().expect("bar news");
    let first = bar_news.try_recv().expect("initial applied news");
    let first_id = first_program_id(&first).expect("initial program id");

    let invalid = "[[item]]\nname = \"Broken\"\nkey = \"ctrl-a\"\nnavo = \"pane.refresh\"\n";
    let news = write_until(&path, invalid, &mut bar_news, |news| {
        matches!(news, ButtonBarNews::Invalid(_))
    });
    match news {
        ButtonBarNews::Invalid(message) => {
            assert!(
                message.contains("line 3"),
                "expected line 3 in error message, got: {message}"
            );
        }
        other => panic!("expected Invalid, got {other:?}"),
    }

    // Previous generation's program must still be kept intact (not refused as stale).
    (bar_hooks.run_program)(first_id, UserInvocation::default());
    let deadline = Instant::now() + NEWS_WAIT;
    let uc_msg = next_user_until(&mut uc_news, deadline).expect("user command news");
    if let UserCommandNews::Refused { message, .. } = uc_msg {
        assert_ne!(
            message,
            navo_l10n::tr("button-bar-stale"),
            "previous program should stay valid after invalid reload"
        );
    }
}

#[test]
fn button_bar_reload_bumps_generation() {
    let (_root, dirs) = temp_dirs();
    let path = dirs.button_bar_file();
    std::fs::write(&path, "[[item]]\nname = \"One\"\nprogram = \"echo\"\n").expect("write v1");

    let (_uc_hooks, _uc_watch, runner) = user_commands::hooks(Some(&dirs));
    let (mut bar_hooks, _watch) = hooks(Some(&dirs), runner);
    let mut bar_news = bar_hooks.news.take().expect("bar news");

    let first = bar_news.try_recv().expect("initial applied");
    let g1 = first_program_id(&first).expect("v1 program id").generation;

    let v2 = "[[item]]\nname = \"Two\"\nprogram = \"echo\"\n";
    let second = write_until(&path, v2, &mut bar_news, |news| {
        first_program_id(news).is_some_and(|id| id.generation > g1)
    });
    let g2 = first_program_id(&second).expect("v2 program id").generation;
    assert_eq!(g2, g1 + 1);
}

#[test]
fn button_bar_stale_program_is_refused() {
    let (_root, dirs) = temp_dirs();
    let path = dirs.button_bar_file();
    std::fs::write(&path, "[[item]]\nname = \"Run\"\nprogram = \"echo\"\n").expect("write v1");

    let (mut uc_hooks, _uc_watch, runner) = user_commands::hooks(Some(&dirs));
    let mut uc_news = uc_hooks.news.take().expect("user command news");
    let _ = uc_news.try_recv();

    let (mut bar_hooks, _watch) = hooks(Some(&dirs), runner);
    let mut bar_news = bar_hooks.news.take().expect("bar news");
    let first = bar_news.try_recv().expect("initial applied");
    let old_id = first_program_id(&first).expect("v1 program id");

    let v2 = "[[item]]\nname = \"Run Two\"\nprogram = \"echo\"\n";
    let _ = write_until(&path, v2, &mut bar_news, |news| {
        first_program_id(news).is_some_and(|id| id.generation > old_id.generation)
    });

    (bar_hooks.run_program)(old_id, UserInvocation::default());
    let deadline = Instant::now() + NEWS_WAIT;
    let refusal = next_user_until(&mut uc_news, deadline).expect("refusal news");
    match refusal {
        UserCommandNews::Refused { message, .. } => {
            assert_eq!(message, navo_l10n::tr("button-bar-stale"));
        }
        other => panic!("expected Refused, got {other:?}"),
    }
}

#[test]
fn button_bar_resolves_icons_off_the_ui_thread() {
    let (_root, dirs) = temp_dirs();
    let path = dirs.button_bar_file();
    std::fs::write(
        &path,
        concat!(
            "[[item]]\n",
            "name = \"Folder\"\n",
            "icon = \"lucide:folder\"\n",
            "navo = \"pane.refresh\"\n\n",
            "[[item]]\n",
            "name = \"beta\"\n",
            "navo = \"pane.refresh\"\n",
        ),
    )
    .expect("write icons buttonbar.toml");

    let (_uc_hooks, _uc_watch, runner) = user_commands::hooks(Some(&dirs));
    let (mut bar_hooks, _watch) = hooks(Some(&dirs), runner);
    let mut bar_news = bar_hooks.news.take().expect("bar news");
    let news = bar_news.try_recv().expect("initial applied");
    let ButtonBarNews::Applied { items, warnings } = news else {
        panic!("expected Applied, got {news:?}");
    };
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(items.len(), 2);

    match &items[0].icon {
        BarIcon::Mono { svg, .. } => {
            let text = std::str::from_utf8(svg).expect("utf-8 svg");
            assert!(text.contains("<svg"), "expected <svg in resolved body");
        }
        other => panic!("expected BarIcon::Mono, got {other:?}"),
    }

    assert_eq!(
        items[1].icon,
        BarIcon::Letters {
            letters: "B".to_owned(),
            shape: Shape::Soft,
            tone: Tone::Muted,
        }
    );
}
