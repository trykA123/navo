use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedReceiver;
use navo_store::StoreDirs;
use navo_ui::window_view::{UserCommandInfo, UserCommandNews, UserInvocation};

use super::{FILE, edit, hooks, model, search};

const NEWS_WAIT: Duration = Duration::from_secs(10);

fn dirs() -> (tempfile::TempDir, StoreDirs) {
    let root = tempfile::tempdir().expect("a temporary folder");
    let dirs = StoreDirs {
        config: root.path().join("config"),
        data: root.path().join("data"),
    };
    dirs.ensure().expect("the settings folders");
    (root, dirs)
}

fn next(receiver: &mut UnboundedReceiver<UserCommandNews>) -> UserCommandNews {
    receiver.try_recv().expect("a news item")
}

fn wait_for(
    receiver: &mut UnboundedReceiver<UserCommandNews>,
    wanted: impl Fn(&UserCommandNews) -> bool,
) -> UserCommandNews {
    let deadline = Instant::now() + NEWS_WAIT;
    loop {
        while let Ok(news) = receiver.try_recv() {
            if wanted(&news) {
                return news;
            }
        }
        assert!(Instant::now() < deadline, "no matching news arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn take(
    receiver: &mut UnboundedReceiver<UserCommandNews>,
    wanted: &impl Fn(&UserCommandNews) -> bool,
) -> Option<UserCommandNews> {
    std::iter::from_fn(|| receiver.try_recv().ok()).find(|news| wanted(news))
}

/// Writes `text` again every 500 ms until the news arrives; the folder watch arms asynchronously on Windows.
fn write_until(
    path: &std::path::Path,
    text: &str,
    receiver: &mut UnboundedReceiver<UserCommandNews>,
    wanted: impl Fn(&UserCommandNews) -> bool,
) -> UserCommandNews {
    let deadline = Instant::now() + NEWS_WAIT;
    loop {
        std::fs::write(path, text).expect("an edit");
        let round = Instant::now() + Duration::from_millis(500);
        while Instant::now() < round {
            if let Some(news) = take(receiver, &wanted) {
                return news;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(Instant::now() < deadline, "no matching news arrived");
    }
}

#[test]
fn a_valid_file_gives_applied() {
    let (_root, dirs) = dirs();
    std::fs::write(
        dirs.user_commands_file(),
        "[[command]]\nname = \"List\"\nprogram = \"tool\"\nargs = \"--version\"\n\
         start-in = \"/work\"\nkey = \"ctrl-alt-l\"\n",
    )
    .expect("a command file");
    let (hooks, _watch) = hooks(Some(&dirs));
    let mut news = hooks.news.expect("news");

    assert_eq!(
        next(&mut news),
        UserCommandNews::Applied(
            vec![UserCommandInfo {
                name: "List".to_owned(),
                key: Some("ctrl-alt-l".to_owned()),
            }],
            Vec::new(),
        )
    );
}

#[test]
fn a_broken_edit_gives_invalid_and_keeps_the_list() {
    let (root, dirs) = dirs();
    let work = root.path().join("work");
    std::fs::create_dir(&work).expect("a work folder");
    let executable = std::env::current_exe().expect("the test program");
    let text = format!(
        "[[command]]\nname = \"Build\"\nprogram = '{}'\nargs = '{{file}}'\nstart-in = '{}'\n",
        executable.display(),
        work.display()
    );
    let path = dirs.user_commands_file();
    std::fs::write(&path, text).expect("a command file");
    let (hooks, _watch) = hooks(Some(&dirs));
    let mut news = hooks.news.expect("news");
    assert!(matches!(
        next(&mut news),
        UserCommandNews::Applied(commands, _) if commands.len() == 1
    ));

    let broken = "[[command]]\nname = \"Broken\"\nprogram = \"{file}\"\n";
    let invalid = write_until(&path, broken, &mut news, |news| {
        matches!(news, UserCommandNews::Invalid(_))
    });
    let UserCommandNews::Invalid(message) = invalid else {
        unreachable!()
    };
    assert!(message.contains(FILE), "{message}");
    assert!(message.contains("line 3"), "{message}");

    (hooks.run)("Build", UserInvocation::default());
    let refused = wait_for(&mut news, |news| {
        matches!(news, UserCommandNews::Refused { .. })
    });
    let UserCommandNews::Refused { name, message } = refused else {
        unreachable!()
    };
    assert_eq!(name, "Build");
    assert!(message.contains("no file under the cursor"), "{message}");
}

#[test]
fn edit_writes_the_template_only_when_the_file_is_absent() {
    let (_root, dirs) = dirs();
    let path = dirs.user_commands_file();

    edit(&path).expect("a template");
    let template = std::fs::read_to_string(&path).expect("the template");
    assert!(template.contains("[[command]]"), "{template}");
    assert!(
        model::parse(&template, &search())
            .expect("a valid template")
            .is_empty(),
        "the template has no active command"
    );

    std::fs::write(&path, "keep me").expect("an existing file");
    edit(&path).expect("an existing file is kept");
    assert_eq!(std::fs::read_to_string(&path).expect("the file"), "keep me");
}

#[cfg(unix)]
#[test]
fn an_exit_news_says_whether_the_console_was_hidden() {
    let (root, dirs) = dirs();
    let work = root.path().join("work");
    std::fs::create_dir(&work).expect("a work folder");
    let mut text = String::new();
    for (name, console) in [("Shown", "show"), ("Hidden", "hide")] {
        text.push_str(&format!(
            "[[command]]\nname = \"{name}\"\nprogram = 'sh'\nargs = '-c \"exit 3\"'\n\
             start-in = '{}'\nconsole = \"{console}\"\n\n",
            work.display()
        ));
    }
    std::fs::write(dirs.user_commands_file(), text).expect("a command file");
    let (hooks, _watch) = hooks(Some(&dirs));
    let mut news = hooks.news.expect("news");
    let file = navo_core::Location::new(work.join("a.txt").to_string_lossy()).expect("a file");
    for (name, hidden) in [("Shown", false), ("Hidden", true)] {
        let invocation = UserInvocation {
            file: Some(file.clone()),
            ..UserInvocation::default()
        };
        (hooks.run)(name, invocation);
        let exited = wait_for(&mut news, |news| {
            matches!(news, UserCommandNews::Exited { .. })
        });
        assert_eq!(
            exited,
            UserCommandNews::Exited {
                name: name.to_owned(),
                code: Some(3),
                hidden,
            }
        );
    }
}
