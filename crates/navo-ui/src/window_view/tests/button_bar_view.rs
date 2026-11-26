use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use navo_icons::spec::{Shape, Tone};

use super::{
    NavoWindow, fake, open_options, options_for, press, settle, two_panes, viewer_open,
    viewer_window,
};
use crate::settings::{Change, Field, Value, Values};
use crate::tokens::{self, Mode};
use crate::window_view::button_bar::view::{runnable, tooltip_text};
use crate::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, OpenHooks,
    ProgramId, SettingsHooks, StartOptions, UserCommandHooks, UserCommandInfo, UserCommandNews,
    UserCommandSender,
};

fn item(name: &str, action: ButtonAction, line: usize) -> ButtonItem {
    ButtonItem {
        name: name.to_owned(),
        icon: BarIcon::Letters {
            letters: name.chars().take(1).collect(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        action,
        line,
    }
}

fn bar_values(shown: bool, names: bool) -> Values {
    Values::default()
        .with(&Change {
            field: Field::WindowButtonBar,
            value: Value::Flag(shown),
        })
        .with(&Change {
            field: Field::WindowButtonBarNames,
            value: Value::Flag(names),
        })
}

fn open_bar(
    cx: &mut TestAppContext,
    mode: Mode,
    (width, height): (f32, f32),
    values: Values,
) -> (
    AnyWindowHandle,
    Entity<NavoWindow>,
    ButtonBarSender,
    UserCommandSender,
) {
    let fs = fake();
    let state = two_panes(&fs);
    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(|_| Ok(())),
        Arc::new(|| Ok(())),
        None,
    );
    let (user_hooks, user_tx) = UserCommandHooks::channel(Arc::new(|_, _| {}), None);
    let options = StartOptions {
        mode,
        settings: SettingsHooks {
            values,
            ..SettingsHooks::none()
        },
        open: OpenHooks {
            button_bar: Some(bar_hooks),
            user_commands: Some(user_hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs, state)
    };
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(mode, cx);
        crate::window_view::bind_app_keys(cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(width), px(height)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(window_options, cx, |window, cx| {
            cx.new(|cx| NavoWindow::new(options, window, cx))
        })
        .expect("open window")
    });
    settle(cx, &view);
    (window, view, bar_tx, user_tx)
}

#[gpui_kit::test]
fn button_bar_view_hidden_when_off(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, _) = open_options(cx, options_for(fs, state));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar").is_none());
    })
    .expect("window");

    let (window_on, _, _, _) = open_bar(cx, Mode::Light, (1400., 900.), bar_values(true, false));
    cx.update_window(window_on, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar").is_some());
        assert!(window.try_find("button-bar-edit").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn button_bar_view_hidden_in_viewer(cx: &mut TestAppContext) {
    let (window, view, _) = viewer_window(cx, Ok("hello\n"), false);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.change_setting(
                Change {
                    field: Field::WindowButtonBar,
                    value: Value::Flag(true),
                },
                window,
                cx,
            );
        });
        window.render_frame(cx);
        assert!(window.try_find("button-bar").is_some());
    })
    .expect("window");

    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    assert!(viewer_open(cx, &view));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar").is_none());
    })
    .expect("window");

    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn button_bar_view_folds_into_more(cx: &mut TestAppContext) {
    let items: Vec<ButtonItem> = (0..10)
        .map(|i| {
            item(
                &format!("Action {i}"),
                ButtonAction::Navo("files.refresh".to_owned()),
                i + 1,
            )
        })
        .collect();

    for mode in [Mode::Light, Mode::Dark] {
        let (narrow, narrow_view, narrow_tx, _) =
            open_bar(cx, mode, (390., 800.), bar_values(true, true));
        assert!(narrow_tx.send(ButtonBarNews::Applied {
            items: items.clone(),
            warnings: Vec::new(),
        }));
        settle(cx, &narrow_view);
        cx.update_window(narrow, |_, window, cx| {
            window.render_frame(cx);
            let mut bar = window.within("button-bar");
            let more = bar.find("button-bar-more");
            assert!(more.visible());
            assert!(bar.try_find(("button-bar-item", 9usize)).is_none());
            assert!(more.bounds().right() <= px(390.));

            bar.click("button-bar-more", cx);
            window.render_frame(cx);
            assert!(window.try_find("button-bar-more-menu").is_some());
            assert!(window.try_find(("button-bar-more-row", 9usize)).is_some());
        })
        .expect("narrow window");

        let (wide, wide_view, wide_tx, _) =
            open_bar(cx, mode, (1400., 900.), bar_values(true, true));
        assert!(wide_tx.send(ButtonBarNews::Applied {
            items: items.clone(),
            warnings: Vec::new(),
        }));
        settle(cx, &wide_view);
        cx.update_window(wide, |_, window, cx| {
            window.render_frame(cx);
            let bar = window.within("button-bar");
            assert!(bar.try_find("button-bar-more").is_none());
            for i in 0..10usize {
                let button = bar.find(("button-bar-item", i));
                assert!(button.visible());
                assert!(button.bounds().right() <= px(1400.));
            }
        })
        .expect("wide window");
    }
}

#[gpui_kit::test]
async fn button_bar_view_tooltip_names_key(cx: &mut TestAppContext) {
    assert!(runnable("files.refresh").is_some());
    assert!(runnable("user_commands.run").is_none());
    assert!(runnable("button_bar.focus").is_none());
    assert!(runnable("viewer.back").is_none());
    assert!(runnable("nope.missing").is_none());

    let user_cmd = item("Build", ButtonAction::User("Build".to_owned()), 1);
    let unknown_navo = item("Ghost", ButtonAction::Navo("nope.missing".to_owned()), 2);
    let commands = vec![UserCommandInfo {
        name: "Build".to_owned(),
        key: Some("ctrl-b".to_owned()),
        takes_files: true,
    }];

    let user_tip = tooltip_text(&user_cmd, &commands);
    assert!(user_tip.contains("Build"), "tip was: {user_tip}");
    assert!(
        user_tip.contains("Ctrl+B") || user_tip.contains("ctrl-b"),
        "tip was: {user_tip}"
    );
    assert!(
        user_tip.contains("Takes dropped files"),
        "tip was: {user_tip}"
    );

    let unknown_tip = tooltip_text(&unknown_navo, &commands);
    assert!(unknown_tip.contains("Ghost"), "tip was: {unknown_tip}");
    assert!(
        unknown_tip.contains("Unknown command"),
        "tip was: {unknown_tip}"
    );

    let program_item = item(
        "Hash",
        ButtonAction::Program {
            id: ProgramId {
                generation: 1,
                index: 0,
            },
            line: "certutil.exe -hashfile {file}".to_owned(),
            takes_files: true,
        },
        3,
    );
    let program_tip = tooltip_text(&program_item, &commands);
    assert!(program_tip.contains("Hash"), "tip was: {program_tip}");
    assert!(
        program_tip.contains("certutil.exe -hashfile {file}"),
        "tip was: {program_tip}"
    );
    assert!(
        program_tip.contains("Takes dropped files"),
        "tip was: {program_tip}"
    );

    let (window, view, bar_tx, user_tx) =
        open_bar(cx, Mode::Light, (1400., 900.), bar_values(true, false));
    assert!(user_tx.send(UserCommandNews::Applied(commands, Vec::new())));
    assert!(bar_tx.send(ButtonBarNews::Applied {
        items: vec![user_cmd, unknown_navo],
        warnings: Vec::new(),
    }));
    settle(cx, &view);

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("button-bar")
            .hover(("button-bar-item", 0usize), cx);
    })
    .expect("hover");
    cx.wait_for(window, Duration::from_secs(2), |window, _| {
        window
            .try_find("tooltip")
            .and_then(|tip| tip.label().map(str::to_owned))
            .is_some_and(|label| label.contains("Build") && label.contains("Takes dropped files"))
    })
    .await;
}
