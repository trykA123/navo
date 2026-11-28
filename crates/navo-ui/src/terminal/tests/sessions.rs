use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Pixels, Point, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use navo_core::AppState;
use navo_term::grid::Color;
use navo_term::{FakeBackend, FakeHandle, Session, ShellKind, ShellSpec, Size};

use crate::primitives::TAB_STATUS_SIZE;
use crate::settings::{Change, Field, Value, Values};
use crate::terminal::{
    SessionSpawner, SessionState, SpawnOptions, TerminalDrawer, TerminalDrawerConfig,
    TerminalSession, spawn_default_session,
};
use crate::tokens::{self, Mode};
use crate::window_view::{NavoWindow, SaveReport, SettingsHooks};

#[derive(Clone)]
struct SpawnRecord {
    shell: ShellSpec,
    cwd: PathBuf,
    handle: FakeHandle,
}

fn shell(kind: ShellKind, program: &str, label: &str) -> ShellSpec {
    ShellSpec {
        kind,
        program: PathBuf::from(program),
        args: Vec::new(),
        label: label.to_owned(),
    }
}

fn recording_spawner() -> (SessionSpawner, Arc<Mutex<Vec<SpawnRecord>>>) {
    let log: Arc<Mutex<Vec<SpawnRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&log);
    let spawner: SessionSpawner = Rc::new(move |spec, cwd, _size, sink| {
        let (backend, handle) = FakeBackend::new(sink);
        capture.lock().unwrap().push(SpawnRecord {
            shell: spec.clone(),
            cwd: cwd.to_path_buf(),
            handle,
        });
        Ok(Session::with_backend(Box::new(backend)))
    });
    (spawner, log)
}

struct DrawerFixture {
    window: AnyWindowHandle,
    drawer: Entity<TerminalDrawer>,
    spawns: Arc<Mutex<Vec<SpawnRecord>>>,
    emptied: Arc<Mutex<bool>>,
}

fn open_configured_drawer(
    cx: &mut TestAppContext,
    width: Pixels,
    configured_shell: &str,
    detected: Vec<ShellSpec>,
    cwd: PathBuf,
) -> DrawerFixture {
    let (spawner, log) = recording_spawner();
    let emptied = Arc::new(Mutex::new(false));
    let emptied_flag = Arc::clone(&emptied);
    let mut drawer_slot = None;
    let configured = configured_shell.to_owned();

    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(Mode::Light, cx);
        crate::window_view::bind_app_keys(cx);
        let options = SpawnOptions {
            cwd: cwd.clone(),
            size: Size { cols: 80, rows: 24 },
            scrollback: navo_term::grid::DEFAULT_SCROLLBACK,
            spawner: Rc::clone(&spawner),
        };
        let initial = spawn_default_session(&configured, &detected, &options, cx);
        let window_opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(width, px(600.0)),
            })),
            ..Default::default()
        };
        cx.open_window(window_opts, |window, cx| {
            let cwd_for_new = cwd.clone();
            let config = TerminalDrawerConfig {
                initial_session: initial,
                detected_shells: detected,
                configured_shell: configured.into(),
                scrollback: navo_term::grid::DEFAULT_SCROLLBACK,
                spawner: Rc::clone(&spawner),
                cwd_provider: Rc::new(move |_| cwd_for_new.clone()),
                on_hide: Rc::new(|_, _| {}),
                on_empty: Rc::new(move |_, _| {
                    *emptied_flag.lock().unwrap() = true;
                }),
            };
            let drawer = cx.new(|cx| TerminalDrawer::with_config(config, cx));
            let focus = drawer.read(cx).focus_handle();
            window.focus(&focus, cx);
            drawer_slot = Some(drawer.clone());
            drawer
        })
        .expect("open drawer window")
    });
    DrawerFixture {
        window: window.into(),
        drawer: drawer_slot.expect("drawer"),
        spawns: log,
        emptied,
    }
}

fn snapshot_row_text(session: &Entity<TerminalSession>, cx: &mut TestAppContext) -> Vec<String> {
    session.read_with(cx, |session, _| {
        session
            .snapshot()
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.c)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    })
}

type SpawnLog = Arc<Mutex<Vec<SpawnRecord>>>;
type ChangeLog = Arc<Mutex<Vec<Change>>>;

fn open_missing_shell_window(
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<NavoWindow>, SpawnLog, ChangeLog) {
    let fs = crate::window_view::tests::fake();
    let folder = fs.location("home");
    let saved_changes: Arc<Mutex<Vec<Change>>> = Arc::new(Mutex::new(Vec::new()));
    let capture_saves = Arc::clone(&saved_changes);

    let initial_values = Values::default()
        .with(&Change {
            field: Field::ModuleTerminal,
            value: Value::Flag(true),
        })
        .with(&Change::text(Field::TerminalShell, "pwsh"));
    let mut options = crate::window_view::tests::options_for(fs, AppState::new(folder));
    options.settings = SettingsHooks {
        values: initial_values.clone(),
        save: Arc::new(move |changes| {
            capture_saves.lock().unwrap().extend_from_slice(changes);
            let next = changes
                .iter()
                .fold(initial_values.clone(), |acc, change| acc.with(change));
            SaveReport::saved(next)
        }),
        ..SettingsHooks::none()
    };

    let (window, view) = crate::window_view::tests::open_options(cx, options);
    let (spawner, spawns) = recording_spawner();
    let detected = vec![
        shell(
            ShellKind::Cmd,
            r"C:\Windows\System32\cmd.exe",
            "Command Prompt",
        ),
        shell(
            ShellKind::GitBash,
            r"C:\Program Files\Git\bin\bash.exe",
            "Git Bash",
        ),
    ];
    view.update(cx, |view: &mut NavoWindow, _| {
        view.set_terminal_detected_shells(detected);
        view.set_terminal_session_spawner(spawner);
    });
    (window, view, spawns, saved_changes)
}

#[gpui_kit::test]
fn missing_configured_shell_starts_fallback_on_enter_without_mutating_setting(
    cx: &mut TestAppContext,
) {
    let (window, view, spawns, saved_changes) = open_missing_shell_window(cx);

    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.simulate_keystrokes(window, "ctrl-`");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let empty = window
            .try_find("terminal-missing-shell")
            .expect("missing pwsh must show the missing-shell empty state");
        assert!(empty.label().is_some_and(|label| label.contains("pwsh")));
        assert!(window.try_find("terminal-missing-start-fallback").is_some());
        let status = window.within("terminal-tabs").find(("tab-status", 0usize));
        assert_eq!(status.bounds().size.width, px(TAB_STATUS_SIZE));
        assert_eq!(status.bounds().size.height, px(TAB_STATUS_SIZE));
    })
    .unwrap();
    assert!(spawns.lock().unwrap().is_empty());

    // Pressing Enter starts the next detected shell (cmd) on the fake backend.
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("terminal-missing-shell").is_none());
    })
    .unwrap();

    let records = spawns.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].shell.kind, ShellKind::Cmd);
    assert_eq!(records[0].shell.label, "Command Prompt");
    drop(records);

    cx.simulate_keystrokes(window, "d i r");
    cx.run_until_parked();
    assert_eq!(spawns.lock().unwrap()[0].handle.written(), b"dir");
    assert!(saved_changes.lock().unwrap().is_empty());
    assert_eq!(
        view.read_with(cx, |view, _| view.terminal_configured_shell().to_owned()),
        "pwsh"
    );
}

fn assert_restarted_scrollback(session: &Entity<TerminalSession>, cx: &mut TestAppContext) {
    let rows_after = snapshot_row_text(session, cx);
    assert!(
        rows_after
            .iter()
            .any(|row| row.contains("prior scrollback line"))
    );
    assert!(rows_after.iter().any(|row| row.contains("Restarted")));
    assert!(
        rows_after
            .iter()
            .any(|row| row.contains("fresh output after restart"))
    );
    let separator_is_muted = session.read_with(cx, |s, _| {
        s.snapshot().rows.iter().any(|row| {
            let text: String = row.iter().map(|c| c.c).collect();
            text.contains("Restarted")
                && row
                    .iter()
                    .filter(|c| c.c != ' ')
                    .all(|c| c.dim && c.fg == Color::Default)
        })
    });
    assert!(
        separator_is_muted,
        "restart separator must use SGR 2 dim cells"
    );
}

#[gpui_kit::test]
fn exited_session_keeps_scrollback_shows_exit_bar_and_restarts_on_enter(cx: &mut TestAppContext) {
    let workdir = PathBuf::from("/workspace/navo-repo");
    let detected = vec![shell(ShellKind::Bash, "/bin/bash", "bash")];
    let DrawerFixture {
        window,
        drawer,
        spawns,
        ..
    } = open_configured_drawer(cx, px(900.0), "", detected, workdir.clone());

    let first_handle = spawns.lock().unwrap()[0].handle.clone();
    first_handle.emit(b"prior scrollback line\r\n");
    first_handle.exit(42);
    cx.run_until_parked();

    let session = drawer.read_with(cx, |d, _| d.active_session().expect("active session"));
    assert_eq!(
        session.read_with(cx, |s, _| s.state()),
        SessionState::Exited(Some(42))
    );

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window
            .try_find("terminal-exit-bar")
            .expect("exited session must show the inline exit bar");
        assert!(bar.label().is_some_and(|label| label.contains("42")));
        assert!(window.try_find("terminal-exit-restart").is_some());
        assert!(window.try_find("terminal-exit-close").is_some());
        let status = window.within("terminal-tabs").find(("tab-status", 0usize));
        assert_eq!(status.bounds().size.width, px(TAB_STATUS_SIZE));
    })
    .unwrap();

    let before = snapshot_row_text(&session, cx);
    assert!(
        before
            .iter()
            .any(|row| row.contains("prior scrollback line"))
    );

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();

    assert_eq!(
        session.read_with(cx, |s, _| s.state()),
        SessionState::Running
    );
    let restarted_handle = {
        let records = spawns.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].cwd, workdir);
        assert_eq!(records[1].shell.label, "bash");
        records[1].handle.clone()
    };

    restarted_handle.emit(b"fresh output after restart\r\n");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("terminal-exit-bar").is_none());
    })
    .unwrap();
    assert_restarted_scrollback(&session, cx);
}

#[gpui_kit::test]
fn multi_session_shortcuts_create_switch_pick_and_close_sessions(cx: &mut TestAppContext) {
    let detected = vec![
        shell(ShellKind::Pwsh, "pwsh.exe", "PowerShell"),
        shell(ShellKind::Cmd, "cmd.exe", "Command Prompt"),
        shell(ShellKind::Wsl, "wsl.exe", "Ubuntu"),
    ];
    let DrawerFixture {
        window,
        drawer,
        spawns,
        emptied,
    } = open_configured_drawer(cx, px(900.0), "", detected, PathBuf::from("/workspace/app"));

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("terminal-new-session").is_some(),
            "header must expose the New session (+) button"
        );
        assert!(
            window.try_find("terminal-shell-picker").is_some(),
            "header must expose the shell picker trigger button"
        );
    })
    .unwrap();

    // Ctrl+Shift+T spawns a second session and activates it.
    cx.simulate_keystrokes(window, "ctrl-shift-t");
    cx.run_until_parked();
    assert_eq!(drawer.read_with(cx, |d, _| d.session_count()), 2);
    assert_eq!(drawer.read_with(cx, |d, _| d.active_index()), 1);

    // Ctrl+Tab cycles forward (wraps 1 -> 0); Ctrl+Shift+Tab cycles backward (0 -> 1).
    cx.simulate_keystrokes(window, "ctrl-tab");
    cx.run_until_parked();
    assert_eq!(drawer.read_with(cx, |d, _| d.active_index()), 0);
    cx.simulate_keystrokes(window, "ctrl-shift-tab");
    cx.run_until_parked();
    assert_eq!(drawer.read_with(cx, |d, _| d.active_index()), 1);

    // Ctrl+Shift+Space opens the shell picker menu; Down + Enter spawns Command Prompt.
    cx.simulate_keystrokes(window, "ctrl-shift-space");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("terminal-shell-picker-menu").is_some(),
            "Ctrl+Shift+Space must open the shell picker popup menu"
        );
        assert!(window.try_find(("terminal-shell-option", 0usize)).is_some());
        assert!(window.try_find(("terminal-shell-option", 1usize)).is_some());
        assert!(window.try_find(("terminal-shell-option", 2usize)).is_some());
    })
    .unwrap();

    cx.simulate_keystrokes(window, "down enter");
    cx.run_until_parked();
    assert_eq!(drawer.read_with(cx, |d, _| d.session_count()), 3);
    assert_eq!(drawer.read_with(cx, |d, _| d.active_index()), 2);
    let picked_label = drawer.read_with(cx, |d, cx| {
        d.active_session()
            .expect("picked session")
            .read(cx)
            .label()
            .clone()
    });
    assert_eq!(picked_label, "Command Prompt");

    // Ctrl+Shift+W shuts down and removes the active session cleanly.
    let picked_handle = spawns.lock().unwrap()[2].handle.clone();
    cx.simulate_keystrokes(window, "ctrl-shift-w");
    cx.run_until_parked();
    assert!(
        picked_handle.is_dropped(),
        "closing a session must shut down its backend"
    );
    assert_eq!(drawer.read_with(cx, |d, _| d.session_count()), 2);
    assert!(!*emptied.lock().unwrap());

    // Closing the remaining two sessions invokes on_empty.
    cx.simulate_keystrokes(window, "ctrl-shift-w ctrl-shift-w");
    cx.run_until_parked();
    assert_eq!(drawer.read_with(cx, |d, _| d.session_count()), 0);
    assert!(
        *emptied.lock().unwrap(),
        "closing the last session must invoke on_empty"
    );
}

#[gpui_kit::test]
fn narrow_width_wraps_header_into_two_rows_without_clipping(cx: &mut TestAppContext) {
    let detected = vec![
        shell(ShellKind::Bash, "/bin/bash", "bash"),
        shell(ShellKind::Zsh, "/bin/zsh", "zsh"),
    ];

    // Wide (>= 760 px): single-row header.
    let wide_win =
        open_configured_drawer(cx, px(900.0), "", detected.clone(), PathBuf::from("/app")).window;
    cx.update_window(wide_win, |_, window, cx| {
        window.render_frame(cx);
        let header = window.find("terminal-drawer-header");
        assert!(
            window.try_find("terminal-header-row-tabs").is_none(),
            "widths >= 760 px must keep a single-row header"
        );
        assert!(
            window.try_find("terminal-header-row-tools").is_none(),
            "widths >= 760 px must not wrap tools to a second row"
        );
        assert_eq!(header.bounds().size.height, px(tokens::HEADER_HEIGHT));
    })
    .unwrap();

    // Narrow (< 760 px): two-row header with zero clipping.
    let narrow_win =
        open_configured_drawer(cx, px(640.0), "", detected, PathBuf::from("/app")).window;
    cx.update_window(narrow_win, |_, window, cx| {
        window.render_frame(cx);
        let header = window.find("terminal-drawer-header");
        let row_tabs = window
            .try_find("terminal-header-row-tabs")
            .expect("widths below 760 px must render a dedicated tabs row");
        let row_tools = window
            .try_find("terminal-header-row-tools")
            .expect("widths below 760 px must render a dedicated tools row");
        assert!(
            row_tools.bounds().origin.y >= row_tabs.bounds().bottom() - px(1.0),
            "tools row must sit below tabs row: tabs={:?} tools={:?}",
            row_tabs.bounds(),
            row_tools.bounds()
        );
        assert!(
            header.bounds().size.height >= px(tokens::HEADER_HEIGHT * 2.0),
            "two-row header height {:?} must fit both rows",
            header.bounds().size.height
        );
        let hide_btn = window.find("terminal-drawer-hide");
        assert!(
            hide_btn.bounds().size.width > px(0.0)
                && hide_btn.bounds().right() <= header.bounds().right() + px(1.0),
            "hide button must stay unclipped inside narrow header: btn={:?} header={:?}",
            hide_btn.bounds(),
            header.bounds()
        );
        let new_btn = window.find("terminal-new-session");
        assert!(
            new_btn.bounds().size.width > px(0.0)
                && new_btn.bounds().right() <= header.bounds().right() + px(1.0),
            "new-session button must stay unclipped inside narrow header"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_last_session_hides_drawer_and_restores_pane_focus(cx: &mut TestAppContext) {
    let fs = crate::window_view::tests::fake();
    let folder = fs.location("home");
    let mut options = crate::window_view::tests::options_for(fs, AppState::new(folder));
    options.settings.values = Values::default().with(&Change {
        field: Field::ModuleTerminal,
        value: Value::Flag(true),
    });
    let (window, view) = crate::window_view::tests::open_options(cx, options);

    let (spawner, spawns) = recording_spawner();
    let detected = vec![shell(ShellKind::Bash, "/bin/bash", "bash")];
    view.update(cx, |view: &mut NavoWindow, _| {
        view.set_terminal_detected_shells(detected);
        view.set_terminal_session_spawner(spawner);
    });

    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.simulate_keystrokes(window, "ctrl-`");
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(view.read_with(cx, |v, _| v.is_terminal_open()));
    assert!(view.read_with(cx, |v, _| v.has_terminal_session()));

    // Close the sole session via Ctrl+Shift+W: drawer hides, session shuts down, pane regains focus.
    cx.simulate_keystrokes(window, "ctrl-shift-w");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();

    assert!(spawns.lock().unwrap()[0].handle.is_dropped());
    assert!(!view.read_with(cx, |v, _| v.is_terminal_open()));
    assert!(!view.read_with(cx, |v, _| v.has_terminal_session()));
    assert!(
        !cx.update_window(window, |_, window, cx| {
            view.read(cx).is_terminal_focused(window, cx)
        })
        .unwrap()
    );
}
