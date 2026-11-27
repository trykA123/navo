use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, ExternalPaths, FileDropEvent,
    InputEvent as _, Point, Role, TestAppContext, WindowBounds, WindowOptions, px, size,
};
use navo_icons::spec::{IconSpec, Shape, Tone};

use super::super::keys::EditButtonBar;
use super::{NavoWindow, fake, home_beside_dest, options_for, settle};
use crate::settings::{Change, Field, Value, Values};
use crate::test_pointer::dispatch;
use crate::tokens::{self, Mode};
use crate::window_view::{
    BarEdit, BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem,
    DraftAction, ImportedIcon, ItemDraft, ItemPath, OpenHooks, ProgramId, SettingsHooks,
    StartOptions,
};

type ImportCalls = Arc<Mutex<Vec<(PathBuf, bool)>>>;
type Edits = Arc<Mutex<Vec<BarEdit>>>;

struct ImportRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    imports: ImportCalls,
    edits: Edits,
}

fn bar_item(name: &str, action: ButtonAction, line: usize) -> ButtonItem {
    ButtonItem {
        name: name.to_owned(),
        icon: BarIcon::Letters {
            letters: name.chars().take(1).collect(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        icon_spec: None,
        action,
        line,
    }
}

fn open_import_rig(
    cx: &mut TestAppContext,
    mode: Mode,
    handler: impl Fn(PathBuf, bool) -> Result<ImportedIcon, String> + Send + Sync + 'static,
) -> ImportRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let imports: ImportCalls = Arc::default();
    let edits: Edits = Arc::default();
    let import_log = Arc::clone(&imports);
    let edits_log = Arc::clone(&edits);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(move |edit| {
            edits_log.lock().expect("edits").push(edit);
            Ok(())
        }),
        Arc::new(|| Ok(())),
        None,
    );
    let bar_hooks = bar_hooks.with_import(Arc::new(move |path, mono| {
        import_log
            .lock()
            .expect("imports")
            .push((path.clone(), mono));
        handler(path, mono)
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

    ImportRig {
        window,
        view,
        bar_tx,
        imports,
        edits,
    }
}

fn open_import_tab(cx: &mut TestAppContext, rig: &ImportRig, items: Vec<ButtonItem>) {
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(EditButtonBar), cx);
    })
    .expect("open editor");
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-picker-tab-import", cx);
        window.render_frame(cx);
    })
    .expect("open import tab");
}

fn drop_on_zone(cx: &mut TestAppContext, rig: &ImportRig, path: PathBuf) {
    let center = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.find("button-bar-import-drop").bounds().center()
        })
        .expect("drop zone center");
    dispatch(
        cx,
        rig.window,
        FileDropEvent::Entered {
            position: center,
            paths: ExternalPaths([path].into_iter().collect()),
        }
        .to_platform_input(),
    );
    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_event(
            FileDropEvent::Submit { position: center }.to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let btn = window.find("button-bar-import-choose");
        assert_eq!(btn.label(), Some("Importing…"));
        assert_eq!(btn.disabled(), Some(true));
    })
    .expect("verify busy state during drop import");
    settle(cx, &rig.view);
}

fn program_item() -> ButtonItem {
    bar_item(
        "Compare",
        ButtonAction::Program {
            id: ProgramId {
                generation: 1,
                index: 0,
            },
            line: "diff.exe {file}".to_owned(),
            start_in: Some("{folder}/work".to_owned()),
            hidden_console: true,
            takes_files: true,
        },
        1,
    )
}

fn assert_program_update(rig: &ImportRig, idx: usize, file: &str, mono: bool) {
    let edits = rig.edits.lock().expect("edits");
    assert_eq!(
        edits[idx],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: ItemDraft {
                name: "Compare".to_owned(),
                action: DraftAction::Program {
                    program: "diff.exe".to_owned(),
                    args: "{file}".to_owned(),
                    start_in: "{folder}/work".to_owned(),
                    hidden_console: true,
                },
                icon: Some(IconSpec::Svg {
                    file: file.to_owned(),
                    mono,
                }),
            },
        }
    );
}

fn choose_import_svg(cx: &mut TestAppContext, rig: &ImportRig, svg: PathBuf) {
    cx.update_window(rig.window, |_, window, cx| {
        assert!(window.find("button-bar-import").visible());
        assert!(window.find("button-bar-import-drop").visible());
        assert!(window.find("button-bar-import-choose").visible());
        assert!(window.find("button-bar-import-keep-colours").visible());
        window.click("button-bar-import-choose", cx);
    })
    .expect("click choose file");

    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![svg.clone()])
    });
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn button_bar_import_sets_icon_and_lists_removed(cx: &mut TestAppContext) {
    let svg_choose = PathBuf::from("icons/custom-a.svg");
    let svg_drop = PathBuf::from("icons/custom-b.svg");
    let rig = open_import_rig(cx, Mode::Light, |path, mono| {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("icon");
        if name == "custom-a" {
            Ok(ImportedIcon {
                spec: IconSpec::Svg {
                    file: "custom-a-1234.svg".to_owned(),
                    mono,
                },
                removed: vec!["text".to_owned(), "filters".to_owned()],
            })
        } else {
            Ok(ImportedIcon {
                spec: IconSpec::Svg {
                    file: "custom-b-5678.svg".to_owned(),
                    mono,
                },
                removed: Vec::new(),
            })
        }
    });
    open_import_tab(cx, &rig, vec![program_item()]);
    choose_import_svg(cx, &rig, svg_choose.clone());

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let cleaned = window.find("button-bar-import-cleaned");
        assert_eq!(cleaned.role(), Some(Role::Status));
        assert_eq!(cleaned.label(), Some("Cleaned: removed text, filters"));
        assert!(window.try_find("button-bar-import-error").is_none());
    })
    .expect("verify cleaned notice");
    assert_eq!(rig.edits.lock().expect("edits").len(), 1);
    assert_program_update(&rig, 0, "custom-a-1234.svg", true);

    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-import-keep-colours", cx);
    })
    .expect("toggle keep colours");
    settle(cx, &rig.view);

    drop_on_zone(cx, &rig, svg_drop.clone());

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-import-cleaned").is_none());
        assert!(window.try_find("button-bar-import-error").is_none());
    })
    .expect("verify second import clears cleaned banner");

    let imports = rig.imports.lock().expect("imports");
    assert_eq!(
        imports.as_slice(),
        &[
            (svg_choose.clone(), true),
            (svg_choose, false),
            (svg_drop, false),
        ]
    );
    assert_eq!(rig.edits.lock().expect("edits").len(), 3);
    assert_program_update(&rig, 2, "custom-b-5678.svg", false);
}

#[gpui_kit::test]
fn button_bar_import_shows_refusal(cx: &mut TestAppContext) {
    let ok_path = PathBuf::from("icons/good.svg");
    let bad_path = PathBuf::from("\\\\server\\share\\bad.svg");
    let rig = open_import_rig(cx, Mode::Dark, |path, mono| {
        if path.to_string_lossy().contains("bad") {
            Err("icon imports must come from a local path".to_owned())
        } else {
            Ok(ImportedIcon {
                spec: IconSpec::Svg {
                    file: "good-1234.svg".to_owned(),
                    mono,
                },
                removed: vec!["gradients".to_owned()],
            })
        }
    });
    let initial = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    open_import_tab(cx, &rig, vec![initial]);

    drop_on_zone(cx, &rig, ok_path.clone());
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("button-bar-import-cleaned").visible());
    })
    .expect("verify initial cleaned");

    drop_on_zone(cx, &rig, bad_path);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let err = window.find("button-bar-import-error");
        assert_eq!(err.role(), Some(Role::Alert));
        assert_eq!(
            err.label(),
            Some("icon imports must come from a local path")
        );
        assert!(window.try_find("button-bar-import-cleaned").is_none());
    })
    .expect("verify refusal alert");
    assert_eq!(rig.edits.lock().expect("edits").len(), 1);

    drop_on_zone(cx, &rig, ok_path);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-import-error").is_none());
        assert!(window.find("button-bar-import-cleaned").visible());
    })
    .expect("verify recovery clears error");
    assert_eq!(rig.edits.lock().expect("edits").len(), 2);
}
