use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use navo_icons::spec::{IconSpec, Shape, TONES, Tone};

use super::super::keys::EditButtonBar;
use super::{NavoWindow, fake, home_beside_dest, options_for, press, settle, type_in};
use crate::settings::{Change, Field, Value, Values};
use crate::tokens::{self, ColorTheme, Mode, Theme, ThemeLook, contrast_ratio};
use crate::window_view::button_bar::icon::tone_colour;
use crate::window_view::{
    BarEdit, BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem,
    DraftAction, ItemDraft, ItemPath, OpenHooks, ProgramId, SettingsHooks, StartOptions,
};

type Edits = Arc<Mutex<Vec<BarEdit>>>;

struct MakerRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
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

fn open_maker_rig(cx: &mut TestAppContext, mode: Mode, (width, height): (f32, f32)) -> MakerRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let edits: Edits = Arc::default();
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
    MakerRig {
        window,
        view,
        bar_tx,
        edits,
    }
}

fn load_and_open_maker(cx: &mut TestAppContext, rig: &MakerRig, items: Vec<ButtonItem>) {
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(EditButtonBar), cx);
    })
    .expect("dispatch EditButtonBar");
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-picker-tab-maker", cx);
        window.render_frame(cx);
    })
    .expect("open maker tab");
}

#[test]
fn button_bar_maker_tone_contrast_check() {
    let failures: Vec<String> = [Mode::Light, Mode::Dark]
        .into_iter()
        .flat_map(|mode| ColorTheme::ALL.into_iter().map(move |ct| (mode, ct)))
        .flat_map(|(mode, color_theme)| {
            let colors = Theme::with_look(
                mode,
                ThemeLook {
                    color_theme,
                    ..ThemeLook::default()
                },
            )
            .colors;
            TONES.into_iter().filter_map(move |tone| {
                let ratio = contrast_ratio(tone_colour(tone, &colors), colors.surface);
                (ratio < 3.0).then(|| format!("{mode:?} {color_theme:?} {tone:?}: {ratio:.2}"))
            })
        })
        .collect();
    assert!(failures.is_empty(), "tone contrast < 3.0: {failures:?}");
}

#[gpui_kit::test]
fn button_bar_maker_sets_letters(cx: &mut TestAppContext) {
    let rig = open_maker_rig(cx, Mode::Light, (1400., 900.));
    let initial = bar_item(
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
    );
    load_and_open_maker(cx, &rig, vec![initial]);

    cx.update_window(rig.window, |_, window, cx| {
        assert!(window.find("button-bar-maker").visible());
        assert!(window.find("button-bar-maker-preview-16").visible());
        assert!(window.find("button-bar-maker-preview-28").visible());
        assert!(window.find("button-bar-maker-preview-48").visible());
        window.click("button-bar-picker-tab-sets", cx);
        window.render_frame(cx);
        assert!(window.find("button-bar-picker").visible());
        window.click("button-bar-picker-tab-maker", cx);
        window.render_frame(cx);
        window.click("button-bar-maker-letters", cx);
    })
    .expect("focus letters input");
    press(cx, rig.window, &rig.view, "ctrl-a");
    type_in(cx, rig.window, &rig.view, " ab ");

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-maker-shape-circle", cx);
        window.render_frame(cx);
        let ok_index = TONES
            .iter()
            .position(|tone| *tone == Tone::Ok)
            .expect("Ok tone");
        window.click(("button-bar-maker-tone", ok_index), cx);
        window.render_frame(cx);
        window.click("button-bar-maker-use", cx);
    })
    .expect("pick shape, tone, and click Use");
    settle(cx, &rig.view);

    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0],
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
                icon: Some(IconSpec::Letters {
                    letters: "AB".to_owned(),
                    shape: Shape::Circle,
                    tone: Tone::Ok,
                }),
            },
        }
    );
}

#[gpui_kit::test]
fn button_bar_maker_sets_glyph_with_dot(cx: &mut TestAppContext) {
    let rig = open_maker_rig(cx, Mode::Dark, (1400., 900.));
    let initial = bar_item("Hash", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_maker(cx, &rig, vec![initial]);

    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-maker-mode-glyph", cx);
        window.render_frame(cx);
        window.click("button-bar-maker-glyph-search", cx);
    })
    .expect("switch to glyph mode");
    type_in(cx, rig.window, &rig.view, "folder");

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let first = window.find(("button-bar-maker-glyph", 0usize));
        assert_eq!(first.label(), Some("folder"));
        window.click(("button-bar-maker-glyph", 0usize), cx);
        window.render_frame(cx);

        let code_index = TONES
            .iter()
            .position(|tone| *tone == Tone::KindCode)
            .expect("KindCode tone");
        let warn_index = TONES
            .iter()
            .position(|tone| *tone == Tone::Warn)
            .expect("Warn tone");
        window.click(("button-bar-maker-glyph-tone", code_index), cx);
        window.render_frame(cx);
        window.click(("button-bar-maker-dot", warn_index), cx);
        window.render_frame(cx);
        window.click("button-bar-maker-use", cx);
    })
    .expect("pick glyph, tone, dot, and click Use");
    settle(cx, &rig.view);

    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: ItemDraft {
                name: "Hash".to_owned(),
                action: DraftAction::Navo("files.refresh".to_owned()),
                icon: Some(IconSpec::Glyph {
                    glyph: "folder".to_owned(),
                    tone: Tone::KindCode,
                    dot: Some(Tone::Warn),
                }),
            },
        }
    );
}
