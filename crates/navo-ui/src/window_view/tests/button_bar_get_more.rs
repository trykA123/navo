use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, Role, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use navo_icons::spec::{Shape, Tone};

use super::super::keys::EditButtonBar;
use super::{NavoWindow, fake, home_beside_dest, options_for, settle};
use crate::settings::{Change, Field, Value, Values};
use crate::tokens::{self, Mode};
use crate::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, OpenHooks,
    SetAction, SetNews, SetRow, SettingsHooks, StartOptions,
};

const INTRO_TEXT: &str = "Downloads come from Navo's pinned GitHub release with a checked SHA-256, go into Navo's data folder, and appear under Downloaded. Remove a set here at any time; buttons that use it fall back to a dashed square.";

struct GetMoreRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    actions: Arc<Mutex<Vec<SetAction>>>,
}

fn bar_item(name: &str) -> ButtonItem {
    ButtonItem {
        name: name.to_owned(),
        icon: BarIcon::Letters {
            letters: name.chars().take(1).collect(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        icon_spec: None,
        action: ButtonAction::Navo("files.refresh".to_owned()),
        line: 1,
    }
}

fn open_get_more_rig(
    cx: &mut TestAppContext,
    mode: Mode,
    handler: impl Fn(SetAction) -> Result<SetNews, String> + Send + Sync + 'static,
) -> GetMoreRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let actions = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&actions);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(|_| Ok(())),
        Arc::new(|| Ok(())),
        None,
    );
    let bar_hooks = bar_hooks.with_sets(Arc::new(move |action| {
        log.lock().expect("actions").push(action.clone());
        handler(action)
    }));

    let values = Values::default().with(&Change {
        field: Field::WindowButtonBar,
        value: Value::Flag(true),
    });
    let options = StartOptions {
        mode,
        settings: SettingsHooks {
            values,
            ..SettingsHooks::none()
        },
        open: OpenHooks {
            button_bar: Some(bar_hooks),
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
                size: size(px(1400.), px(900.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(window_options, cx, |window, cx| {
            cx.new(|cx| NavoWindow::new(options, window, cx))
        })
        .expect("open window")
    });
    settle(cx, &view);

    assert!(bar_tx.send(ButtonBarNews::Applied {
        items: vec![bar_item("Refresh")],
        warnings: Vec::new(),
    }));
    settle(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(EditButtonBar), cx);
    })
    .expect("open editor");
    settle(cx, &view);

    GetMoreRig {
        window,
        view,
        bar_tx,
        actions,
    }
}

fn open_get_more_dialog(cx: &mut TestAppContext, rig: &GetMoreRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-picker-get-more", cx);
    })
    .expect("click get more");
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("render get more");
}

#[gpui_kit::test]
fn button_bar_get_more_lists_and_gets(cx: &mut TestAppContext) {
    let rig = open_get_more_rig(cx, Mode::Light, |action| match action {
        SetAction::List => Ok(SetNews::Listed(vec![
            SetRow {
                id: "fluent".to_owned(),
                name: "Fluent".to_owned(),
                licence: "MIT".to_owned(),
                size: 1_258_291,
                installed: false,
            },
            SetRow {
                id: "carbon".to_owned(),
                name: "Carbon".to_owned(),
                licence: "Apache-2.0".to_owned(),
                size: 460_800,
                installed: false,
            },
        ])),
        SetAction::Get(id) if id == "fluent" => Ok(SetNews::Installed(id)),
        SetAction::Get(id) if id == "carbon" => Err("Download failed (error 12007)".to_owned()),
        other => Err(format!("unexpected action {other:?}")),
    });
    let _keep_tx = &rig.bar_tx;

    open_get_more_dialog(cx, &rig);
    verify_listed_rows(cx, &rig);
    verify_get_fluent(cx, &rig);
    verify_get_carbon_error_and_back(cx, &rig);

    let actions = rig.actions.lock().expect("actions");
    assert_eq!(
        actions.as_slice(),
        &[
            SetAction::List,
            SetAction::Get("fluent".to_owned()),
            SetAction::Get("carbon".to_owned()),
        ]
    );
}

fn verify_listed_rows(cx: &mut TestAppContext, rig: &GetMoreRig) {
    cx.update_window(rig.window, |_, window, _cx| {
        assert!(window.find("button-bar-get-more-view").visible());
        assert_eq!(
            window.find("button-bar-get-more-intro").label(),
            Some(INTRO_TEXT)
        );
        let row0 = window.find(("button-bar-get-more-row", 0usize));
        let row0_label = row0.label().expect("row0 label");
        assert!(row0_label.contains("Fluent"), "{row0_label}");
        assert!(row0_label.contains("MIT"), "{row0_label}");
        assert!(row0_label.contains("1.2 MB"), "{row0_label}");

        let row1 = window.find(("button-bar-get-more-row", 1usize));
        let row1_label = row1.label().expect("row1 label");
        assert!(row1_label.contains("Carbon"), "{row1_label}");
        assert!(row1_label.contains("Apache-2.0"), "{row1_label}");
        assert!(row1_label.contains("450.0 KB"), "{row1_label}");

        assert_eq!(
            window.find(("button-bar-get-more-action", 0usize)).label(),
            Some("Get")
        );
    })
    .expect("verify listed rows");
}

fn verify_get_fluent(cx: &mut TestAppContext, rig: &GetMoreRig) {
    let gen_before = rig
        .view
        .read_with(cx, |view, _| view.button_bar.picker_generation);

    cx.update_window(rig.window, |_, window, cx| {
        window.click(("button-bar-get-more-action", 0usize), cx);
        window.render_frame(cx);
        let btn = window.find(("button-bar-get-more-action", 0usize));
        assert_eq!(btn.label(), Some("Downloading…"));
        assert_eq!(btn.disabled(), Some(true));
    })
    .expect("verify downloading state");

    settle(cx, &rig.view);

    let gen_after = rig
        .view
        .read_with(cx, |view, _| view.button_bar.picker_generation);
    assert!(
        gen_after > gen_before,
        "expected TagIndex reload after install"
    );

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let btn = window.find(("button-bar-get-more-action", 0usize));
        assert_eq!(btn.label(), Some("Remove"));
        assert_ne!(btn.disabled(), Some(true));
        assert!(
            window
                .try_find(("button-bar-get-more-error", 0usize))
                .is_none()
        );
    })
    .expect("verify fluent installed");
}

fn verify_get_carbon_error_and_back(cx: &mut TestAppContext, rig: &GetMoreRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.click(("button-bar-get-more-action", 1usize), cx);
    })
    .expect("click get carbon");

    settle(cx, &rig.view);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let err = window.find(("button-bar-get-more-error", 1usize));
        assert_eq!(err.role(), Some(Role::Alert));
        assert_eq!(err.label(), Some("Download failed (error 12007)"));
        let btn = window.find(("button-bar-get-more-action", 1usize));
        assert_eq!(btn.label(), Some("Get"));
        assert_ne!(btn.disabled(), Some(true));

        window.click("button-bar-get-more-back", cx);
        window.render_frame(cx);
        assert!(window.try_find("button-bar-get-more-view").is_none());
        assert!(window.find("button-bar-picker").visible());
    })
    .expect("verify error and back button");
}

#[gpui_kit::test]
fn button_bar_get_more_remove(cx: &mut TestAppContext) {
    let rig = open_get_more_rig(cx, Mode::Dark, |action| match action {
        SetAction::List => Ok(SetNews::Listed(vec![
            SetRow {
                id: "fluent".to_owned(),
                name: "Fluent".to_owned(),
                licence: "MIT".to_owned(),
                size: 1_258_291,
                installed: true,
            },
            SetRow {
                id: "carbon".to_owned(),
                name: "Carbon".to_owned(),
                licence: "Apache-2.0".to_owned(),
                size: 460_800,
                installed: true,
            },
        ])),
        SetAction::Remove(id) if id == "fluent" => Ok(SetNews::Removed(id)),
        SetAction::Remove(id) if id == "carbon" => Err("Could not remove pack".to_owned()),
        other => Err(format!("unexpected action {other:?}")),
    });
    let _keep_tx = &rig.bar_tx;

    open_get_more_dialog(cx, &rig);

    let gen_before = rig
        .view
        .read_with(cx, |view, _| view.button_bar.picker_generation);

    cx.update_window(rig.window, |_, window, cx| {
        assert_eq!(
            window.find(("button-bar-get-more-action", 0usize)).label(),
            Some("Remove")
        );
        window.click(("button-bar-get-more-action", 0usize), cx);
        window.render_frame(cx);
        let btn = window.find(("button-bar-get-more-action", 0usize));
        assert_eq!(btn.disabled(), Some(true));
    })
    .expect("verify busy during remove");

    settle(cx, &rig.view);

    let gen_after = rig
        .view
        .read_with(cx, |view, _| view.button_bar.picker_generation);
    assert!(
        gen_after > gen_before,
        "expected TagIndex reload after remove"
    );

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let btn = window.find(("button-bar-get-more-action", 0usize));
        assert_eq!(btn.label(), Some("Get"));
        assert_ne!(btn.disabled(), Some(true));

        window.click(("button-bar-get-more-action", 1usize), cx);
    })
    .expect("click remove carbon");

    settle(cx, &rig.view);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let err = window.find(("button-bar-get-more-error", 1usize));
        assert_eq!(err.role(), Some(Role::Alert));
        assert_eq!(err.label(), Some("Could not remove pack"));
        let btn = window.find(("button-bar-get-more-action", 1usize));
        assert_eq!(btn.label(), Some("Remove"));
        assert_ne!(btn.disabled(), Some(true));
    })
    .expect("verify remove error keeps installed state");

    let actions = rig.actions.lock().expect("actions");
    assert_eq!(
        actions.as_slice(),
        &[
            SetAction::List,
            SetAction::Remove("fluent".to_owned()),
            SetAction::Remove("carbon".to_owned()),
        ]
    );
}
