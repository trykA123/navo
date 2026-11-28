use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::base::Scrollbar;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use navo_core::Location;
use navo_icons::registry::NOTICE_TXT;
use navo_icons::spec::{BrandColour, IconSpec, Shape, Tone};

use super::super::keys::EditButtonBar;
use super::{NavoWindow, fake, home_beside_dest, options_for, press, settle, type_in};
use crate::primitives::{GPU_IMAGES, read_tally};
use crate::settings::{Change, Field, Value, Values};
use crate::tokens::{self, Mode};
use crate::window_view::button_bar::raster_cache::{MAX_PICKER_CACHE_ENTRIES, PickerRasterCache};
use crate::window_view::{
    BarEdit, BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem,
    DraftAction, ItemDraft, ItemPath, OpenHooks, ProgramId, ResidentHooks, SettingsHooks,
    StartOptions, WindowShown,
};

type Edits = Arc<Mutex<Vec<BarEdit>>>;

struct PickerRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    edits: Edits,
    sample_file: Location,
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

fn open_picker_rig(cx: &mut TestAppContext, mode: Mode, size_px: (f32, f32)) -> PickerRig {
    open_picker_rig_with(cx, mode, size_px, &[], None, None)
}

fn open_picker_rig_with(
    cx: &mut TestAppContext,
    mode: Mode,
    (width, height): (f32, f32),
    extra_settings: &[Change],
    window_shown: Option<WindowShown>,
    resident: Option<Rc<dyn ResidentHooks>>,
) -> PickerRig {
    let fs = fake();
    let sample_file = fs.location("home/a.txt");
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
    let mut values = Values::default().with(&Change {
        field: Field::WindowButtonBar,
        value: Value::Flag(true),
    });
    for change in extra_settings {
        values = values.with(change);
    }
    let options = StartOptions {
        mode,
        settings: SettingsHooks {
            values,
            ..SettingsHooks::none()
        },
        open: OpenHooks {
            button_bar: Some(bar_hooks),
            window_shown,
            resident,
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
    PickerRig {
        window,
        view,
        bar_tx,
        edits,
        sample_file,
    }
}

fn load_and_open_editor(cx: &mut TestAppContext, rig: &PickerRig, items: Vec<ButtonItem>) {
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
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("render editor");
}

#[gpui_kit::test]
fn button_bar_picker_searches_and_sets_icon(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Light, (1400., 900.));
    let initial = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![initial]);

    verify_lucide_pick(cx, &rig);
    verify_brand_pick(cx, &rig);
}

fn verify_lucide_pick(cx: &mut TestAppContext, rig: &PickerRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("button-bar-picker-search").visible());
        window.click("button-bar-picker-filter-lucide", cx);
        window.render_frame(cx);
        window.click("button-bar-picker-search", cx);
    })
    .expect("focus picker search");
    type_in(cx, rig.window, &rig.view, "folder");

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let first = window.find(("button-bar-picker-icon", 0usize));
        assert_eq!(first.label(), Some("lucide:folder"));
        window.click(("button-bar-picker-icon", 0usize), cx);
    })
    .expect("click lucide:folder");
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
                name: "Refresh".to_owned(),
                action: DraftAction::Navo("files.refresh".to_owned()),
                icon: Some(IconSpec::Set {
                    set: "lucide".to_owned(),
                    name: "folder".to_owned(),
                    tone: None,
                }),
            },
        }
    );
}

fn verify_brand_pick(cx: &mut TestAppContext, rig: &PickerRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-picker-filter-brands", cx);
        window.render_frame(cx);
        assert!(window.find("button-bar-picker-brand-colour").visible());
        window.click("button-bar-picker-search", cx);
    })
    .expect("switch to brands");
    press(cx, rig.window, &rig.view, "ctrl-a");
    type_in(cx, rig.window, &rig.view, "github");

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let first = window.find(("button-bar-picker-icon", 0usize));
        assert_eq!(first.label(), Some("brands:github"));
        window.click(("button-bar-picker-icon", 0usize), cx);
    })
    .expect("click brands:github theme");
    settle(cx, &rig.view);

    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-picker-brand-colour", cx);
        window.render_frame(cx);
        window.click(("button-bar-picker-icon", 0usize), cx);
    })
    .expect("toggle brand colour and click brands:github");
    settle(cx, &rig.view);

    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 3);
    assert_eq!(
        edits[1],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: ItemDraft {
                name: "Refresh".to_owned(),
                action: DraftAction::Navo("files.refresh".to_owned()),
                icon: Some(IconSpec::Brand {
                    name: "github".to_owned(),
                    colour: BrandColour::Theme,
                }),
            },
        }
    );
    assert_eq!(
        edits[2],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: ItemDraft {
                name: "Refresh".to_owned(),
                action: DraftAction::Navo("files.refresh".to_owned()),
                icon: Some(IconSpec::Brand {
                    name: "github".to_owned(),
                    colour: BrandColour::Brand,
                }),
            },
        }
    );
}

#[gpui_kit::test]
fn button_bar_picker_drops_index_on_close(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Dark, (1400., 900.));
    let initial = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![initial]);

    let loaded = rig.view.read_with(cx, |view, _| {
        view.button_bar
            .editor
            .as_ref()
            .is_some_and(|editor| editor.picker.has_index())
    });
    assert!(loaded, "TagIndex should load while editor is open");

    press(cx, rig.window, &rig.view, "escape");
    let closed = rig
        .view
        .read_with(cx, |view, _| view.button_bar.editor.is_none());
    assert!(closed, "closing editor should drop editor and TagIndex");

    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_action(Box::new(EditButtonBar), cx);
    })
    .expect("open editor");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .expect("immediately close editor");
    settle(cx, &rig.view);
    let still_closed = rig
        .view
        .read_with(cx, |view, _| view.button_bar.editor.is_none());
    assert!(
        still_closed,
        "stale background TagIndex load must be ignored"
    );

    open_editor_again(cx, &rig);
    let reloaded = rig.view.read_with(cx, |view, _| {
        view.button_bar
            .editor
            .as_ref()
            .is_some_and(|editor| editor.picker.has_index())
    });
    assert!(reloaded, "reopening after trim reloads TagIndex");
}

fn open_editor_again(cx: &mut TestAppContext, rig: &PickerRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(EditButtonBar), cx);
    })
    .expect("reopen editor");
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn button_bar_picker_shows_licence_per_group(cx: &mut TestAppContext) {
    for mode in [Mode::Light, Mode::Dark] {
        let rig = open_picker_rig(cx, mode, (1400., 900.));
        let initial = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
        load_and_open_editor(cx, &rig, vec![initial]);

        cx.update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let group0 = window.find(("button-bar-picker-group", 0usize));
            let label = group0.label().expect("group header label");
            assert!(label.contains("Lucide"), "expected Lucide in {label}");
            assert!(label.contains("ISC"), "expected ISC in {label}");
            assert!(
                !label.contains("48 /"),
                "All filter should show full uncapped Lucide set, got {label}"
            );
            assert!(
                window
                    .try_find(("button-bar-picker-more", 7usize))
                    .is_none(),
                "All filter should scroll freely without a 48-icon dead-end note"
            );
            assert!(
                window.try_find("button-bar-picker-brand-colour").is_none(),
                "All filter should exclude Brands logos"
            );

            window.click("button-bar-picker-filter-brands", cx);
            window.render_frame(cx);
            let brands_group = window.find(("button-bar-picker-group", 0usize));
            let brands_label = brands_group.label().expect("brands group label");
            assert!(
                brands_label.contains("Brands") && brands_label.contains("CC0"),
                "expected Brands + CC0 in {brands_label}"
            );
            assert!(window.find("button-bar-picker-brand-colour").visible());

            window.click("button-bar-picker-filter-tabler", cx);
            window.render_frame(cx);
            let tabler_group = window.find(("button-bar-picker-group", 0usize));
            let tabler_label = tabler_group.label().expect("tabler group label");
            assert!(
                tabler_label.contains("Tabler") && tabler_label.contains("MIT"),
                "expected Tabler + MIT in {tabler_label}"
            );
            assert!(
                window
                    .try_find(("button-bar-picker-more", 7usize))
                    .is_none(),
                "set filter should show all hits without cap note"
            );

            window.click("button-bar-picker-licences", cx);
            window.render_frame(cx);
            assert!(window.find("button-bar-picker-licences-view").visible());
            let notice = window.find("button-bar-picker-licences-text");
            assert_eq!(notice.label(), Some(NOTICE_TXT));

            window.click("button-bar-picker-licences-back", cx);
            window.render_frame(cx);
            assert!(window.try_find("button-bar-picker-licences-view").is_none());
            assert!(window.find("button-bar-picker-list").visible());
        })
        .expect("picker licences and groups");
    }
}

#[gpui_kit::test]
fn button_bar_picker_preserves_program_fields_and_existing_icon(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Light, (1400., 900.));
    let initial_icon = IconSpec::Set {
        set: "lucide".to_owned(),
        name: "play".to_owned(),
        tone: None,
    };
    let mut prog_item = bar_item(
        "Run",
        ButtonAction::Program {
            id: ProgramId {
                generation: 1,
                index: 0,
            },
            line: "tool.exe --flag".to_owned(),
            start_in: Some("{folder}/sub".to_owned()),
            hidden_console: true,
            takes_files: false,
        },
        1,
    );
    prog_item.icon_spec = Some(initial_icon.clone());

    let existing_navo_icon = IconSpec::Set {
        set: "lucide".to_owned(),
        name: "home".to_owned(),
        tone: None,
    };
    let mut navo_item = bar_item("Home", ButtonAction::Navo("files.refresh".to_owned()), 2);
    navo_item.icon_spec = Some(existing_navo_icon.clone());

    load_and_open_editor(cx, &rig, vec![prog_item.clone(), navo_item.clone()]);

    pick_first_lucide_folder(cx, &rig);
    let updated_icon = IconSpec::Set {
        set: "lucide".to_owned(),
        name: "folder".to_owned(),
        tone: None,
    };
    assert_program_update(&rig, 0, "Run", &updated_icon);

    prog_item.icon_spec = Some(updated_icon.clone());
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items: vec![prog_item, navo_item],
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);

    rename_selected_button(cx, &rig, "!");
    assert_program_update(&rig, 1, "Run!", &updated_icon);

    cx.update_window(rig.window, |_, window, cx| {
        window.click(("button-bar-editor-tile", 1usize), cx);
        window.render_frame(cx);
    })
    .expect("select second item");
    rename_selected_button(cx, &rig, "s");
    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 3);
    assert_eq!(
        edits[2],
        BarEdit::Update {
            at: ItemPath {
                index: 1,
                child: None,
            },
            draft: ItemDraft {
                name: "Homes".to_owned(),
                action: DraftAction::Navo("files.refresh".to_owned()),
                icon: Some(existing_navo_icon),
            },
        }
    );
}

fn pick_first_lucide_folder(cx: &mut TestAppContext, rig: &PickerRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-picker-filter-lucide", cx);
        window.render_frame(cx);
        window.click("button-bar-picker-search", cx);
    })
    .expect("focus picker search");
    type_in(cx, rig.window, &rig.view, "folder");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(("button-bar-picker-icon", 0usize), cx);
    })
    .expect("pick folder icon");
    settle(cx, &rig.view);
}

fn rename_selected_button(cx: &mut TestAppContext, rig: &PickerRig, suffix: &str) {
    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-form-name", cx);
    })
    .expect("focus form name");
    type_in(cx, rig.window, &rig.view, suffix);
    cx.executor().advance_clock(Duration::from_millis(450));
    settle(cx, &rig.view);
}

fn assert_program_update(rig: &PickerRig, idx: usize, name: &str, icon: &IconSpec) {
    let edits = rig.edits.lock().expect("edits");
    assert_eq!(
        edits[idx],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: ItemDraft {
                name: name.to_owned(),
                action: DraftAction::Program {
                    program: "tool.exe".to_owned(),
                    args: "--flag".to_owned(),
                    start_in: "{folder}/sub".to_owned(),
                    hidden_console: true,
                },
                icon: Some(icon.clone()),
            },
        }
    );
}

#[gpui_kit::test]
fn button_bar_picker_handles_downloaded_empty_and_separator_guard(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Light, (1400., 900.));
    let button = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    let separator = ButtonItem {
        name: String::new(),
        icon: BarIcon::Missing { dashed: false },
        icon_spec: None,
        action: ButtonAction::Separator,
        line: 2,
    };
    load_and_open_editor(cx, &rig, vec![button, separator]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-picker-filter-downloaded", cx);
        window.render_frame(cx);
        assert!(window.find("button-bar-picker").visible());
        assert!(window.try_find("button-bar-picker-list").is_none());

        window.click(("button-bar-editor-tile", 1usize), cx);
        window.render_frame(cx);
        assert!(window.find("button-bar-editor-icon-pane").visible());
        assert!(window.try_find("button-bar-picker").is_none());
    })
    .expect("downloaded empty and separator guard");
}

#[gpui_kit::test]
fn button_bar_editor_lists_clear_scrollbar_gutter(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Dark, (1400., 900.));
    let button = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![button]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let picker_list = window.find("button-bar-picker-list").bounds();
        let licence = window.find(("button-bar-picker-licence", 0usize)).bounds();
        assert!(
            licence.right() <= picker_list.right() - Scrollbar::width(),
            "picker licence pill {:?} overlaps scrollbar gutter of {:?}",
            licence.right(),
            picker_list.right() - Scrollbar::width(),
        );

        let navo_list = window.find("button-bar-form-navo-list").bounds();
        let cmd_id = window.find(("button-bar-form-secondary", 0usize)).bounds();
        assert!(
            cmd_id.right() <= navo_list.right() - Scrollbar::width(),
            "navo command id {:?} overlaps scrollbar gutter of {:?}",
            cmd_id.right(),
            navo_list.right() - Scrollbar::width(),
        );
    })
    .expect("scrollbar gutter check");
}

#[gpui_kit::test]
fn button_bar_picker_caches_bounded_previews_and_clears_on_close(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Dark, (1400., 900.));
    let button = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![button]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("render open picker");
    cx.run_until_parked();

    let cached_open = cx.read(|cx| {
        cx.try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len)
    });
    assert!(
        cached_open > 0 && cached_open <= MAX_PICKER_CACHE_ENTRIES,
        "open picker should populate bounded PickerRasterCache, got {cached_open}"
    );

    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-picker-filter-tabler", cx);
        window.render_frame(cx);
        window.click("button-bar-picker-tab-maker", cx);
        window.render_frame(cx);
        window.click("button-bar-maker-mode-glyph", cx);
        window.render_frame(cx);
    })
    .expect("switch to tabler and maker glyph grid");
    cx.run_until_parked();

    let cached_maker = cx.read(|cx| {
        cx.try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len)
    });
    assert!(
        cached_maker > 0 && cached_maker <= MAX_PICKER_CACHE_ENTRIES,
        "maker glyph grid should populate bounded PickerRasterCache, got {cached_maker}"
    );

    press(cx, rig.window, &rig.view, "escape");
    cx.run_until_parked();

    cx.read(|cx| {
        let remaining = cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len);
        assert_eq!(
            remaining, 0,
            "close_button_bar_editor must clear PickerRasterCache"
        );
        assert_eq!(
            read_tally(&GPU_IMAGES, cx).items,
            0,
            "close_button_bar_editor must release all picker GPU images"
        );
    });
}

struct PickerFakeResident;

impl ResidentHooks for PickerFakeResident {
    fn apply(&self, _values: &Values) {}

    fn key_taken(&self) -> bool {
        false
    }

    fn clipboard_key_taken(&self) -> bool {
        false
    }

    fn has_tray(&self) -> bool {
        true
    }
}

#[gpui_kit::test]
fn button_bar_picker_tall_scaled_tabler_has_zero_single_frame_evictions(cx: &mut TestAppContext) {
    let rig = open_picker_rig_with(
        cx,
        Mode::Dark,
        (1400., 960.),
        &[Change {
            field: Field::UiScale,
            value: Value::Number(80),
        }],
        None,
        None,
    );
    let button = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![button]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-picker-filter-tabler", cx);
        cx.default_global::<PickerRasterCache>().reset_evictions();
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("render tabler at 80% scale");
    cx.run_until_parked();

    cx.read(|cx| {
        let cache = cx
            .try_global::<PickerRasterCache>()
            .expect("picker cache should exist");
        assert!(
            cache.len() > 96,
            "expected > 96 visible Tabler icons at 80% scale in a tall viewport, got {}",
            cache.len()
        );
        assert!(
            cache.len() <= MAX_PICKER_CACHE_ENTRIES,
            "cache len {} exceeded cap {MAX_PICKER_CACHE_ENTRIES}",
            cache.len()
        );
        assert_eq!(
            cache.evictions(),
            0,
            "single-frame Tabler viewport + redraw must incur 0 cache evictions"
        );
    });
}

#[gpui_kit::test]
fn button_bar_picker_clears_cache_on_hide_to_tray_and_window_close(cx: &mut TestAppContext) {
    let hidden = Arc::new(Mutex::new(Vec::new()));
    let hidden_log = Arc::clone(&hidden);
    let rig = open_picker_rig_with(
        cx,
        Mode::Dark,
        (1400., 900.),
        &[Change {
            field: Field::KeepInTray,
            value: Value::Flag(true),
        }],
        Some(Arc::new(move |_, shown| {
            hidden_log.lock().expect("hidden").push(shown);
            true
        })),
        Some(Rc::new(PickerFakeResident)),
    );
    let button = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![button]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("render open picker");
    cx.run_until_parked();
    assert!(
        cx.read(|cx| cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len))
            > 0,
        "picker cache should be populated before hide-to-tray"
    );

    let closed = cx
        .update_window(rig.window, |_, window, cx| {
            rig.view
                .update(cx, |view, cx| view.close_or_hide(window, cx))
        })
        .expect("close_or_hide");
    cx.run_until_parked();
    assert!(!closed, "keep-in-tray should hide rather than close");
    assert_eq!(*hidden.lock().expect("hidden"), vec![false]);
    cx.read(|cx| {
        let remaining = cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len);
        assert_eq!(remaining, 0, "hide-to-tray must clear PickerRasterCache");
        assert_eq!(
            read_tally(&GPU_IMAGES, cx).items,
            0,
            "hide-to-tray must release all picker GPU images"
        );
    });

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("re-render open picker");
    cx.run_until_parked();
    assert!(
        cx.read(|cx| cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len))
            > 0,
        "picker cache should repopulate before window close"
    );

    let closed = cx
        .update_window(rig.window, |_, window, cx| {
            rig.view.update(cx, |view, cx| view.close(window, cx))
        })
        .expect("close");
    cx.run_until_parked();
    assert!(closed, "window close should succeed");
    cx.read(|cx| {
        let remaining = cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len);
        assert_eq!(remaining, 0, "window close must clear PickerRasterCache");
        assert_eq!(
            read_tally(&GPU_IMAGES, cx).items,
            0,
            "window close must release all picker GPU images"
        );
    });
}

#[gpui_kit::test]
fn button_bar_picker_clears_cache_when_viewer_opens(cx: &mut TestAppContext) {
    let rig = open_picker_rig(cx, Mode::Dark, (1400., 900.));
    let button = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    load_and_open_editor(cx, &rig, vec![button]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("render open picker");
    cx.run_until_parked();
    assert!(
        cx.read(|cx| cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len))
            > 0,
        "picker cache should be populated before viewer opens"
    );

    let file = rig.sample_file.clone();
    cx.update_window(rig.window, |_, window, cx| {
        rig.view
            .update(cx, |view, cx| view.view_location(file, window, cx));
    })
    .expect("open viewer over editor");
    cx.run_until_parked();

    assert!(rig.view.read_with(cx, |view, _| view.viewer.is_some()));
    cx.read(|cx| {
        let remaining = cx
            .try_global::<PickerRasterCache>()
            .map_or(0, PickerRasterCache::len);
        assert_eq!(
            remaining, 0,
            "show_viewer must clear PickerRasterCache when covering the editor"
        );
        assert_eq!(
            read_tally(&GPU_IMAGES, cx).items,
            0,
            "show_viewer must release all picker GPU images"
        );
    });
}
