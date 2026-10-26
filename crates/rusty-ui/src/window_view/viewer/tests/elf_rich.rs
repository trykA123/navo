use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_automotive::elf_sample::{Named, build, demo, firmware, func};

use super::structured::fake_with;
use super::{Log, Setup, hooks, label, press_only, redraw, text_of, visible, window_with};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::space::DEMO_MDF;
use crate::window_view::viewer::{FileHooks, Head, Kinds};

fn with_mdf(files: FileHooks, mdf: &'static str) -> FileHooks {
    let (head, bytes) = (files.head.clone(), files.bytes.clone());
    FileHooks {
        head: Arc::new(move |location| {
            if location.as_str().ends_with(".mdf") {
                return Ok(Head {
                    bytes: mdf.as_bytes().to_vec(),
                    len: mdf.len() as u64,
                });
            }
            if location.as_str().ends_with(".elf") {
                return head(location);
            }
            Err("not found".to_owned())
        }),
        bytes: Arc::new(move |location, limit| {
            if location.as_str().ends_with(".mdf") {
                return Ok(mdf.as_bytes().to_vec());
            }
            bytes(location, limit)
        }),
        ..files
    }
}

pub(in crate::window_view::viewer) fn settle_all(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, view);
        redraw(cx, window);
    }
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_bytes(cx, demo().expect("the demo builds"))
}

#[gpui_kit::test]
fn configured_module_prefixes_regroup_a_live_elf(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    let values = preset(1).with(&Change::text(Field::AutomotiveModules, "Can_MainFunction"));
    show(cx, (window, &view), values);
    assert!(has_group(cx, window, "Can_MainFunction"));
    assert!(text_of(cx, window, "space-provenance").ends_with("modules: 1 from your list"));
    show(cx, (window, &view), preset(1));
    assert!(!has_group(cx, window, "Can_MainFunction"));
    assert!(!text_of(cx, window, "space-provenance").contains("from your list"));
}

pub(super) fn has_group(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> bool {
    for at in 0..22 {
        if !has(cx, window, ("treemap-tile", at)) {
            return false;
        }
        if label(cx, window, ("treemap-tile", at)).starts_with(&format!("{name}, ")) {
            return true;
        }
    }
    false
}

fn open_bytes(cx: &mut TestAppContext, bytes: Vec<u8>) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_in(cx, bytes, None)
}

pub(in crate::window_view::viewer) fn open_in(
    cx: &mut TestAppContext,
    bytes: Vec<u8>,
    values: Option<Values>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_with_mdf(cx, bytes, DEMO_MDF, values)
}

pub(in crate::window_view::viewer) fn open_with_mdf(
    cx: &mut TestAppContext,
    bytes: Vec<u8>,
    mdf: &'static str,
    values: Option<Values>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let len = bytes.len() as u64;
    let files = with_mdf(hooks(Setup::new(bytes), &Log::default()), mdf);
    let (window, view) = window_with(cx, fake_with("app.elf", len), files, Kinds::builtin());
    if let Some(values) = values {
        show(cx, (window, &view), values);
    }
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    settle_all(cx, window, &view);
    (window, view)
}

fn flag(field: Field, on: bool) -> Change {
    Change {
        field,
        value: Value::Flag(on),
    }
}

pub(in crate::window_view::viewer) fn preset(at: usize) -> Values {
    Values::default().with(&Change {
        field: Field::AutomotivePreset,
        value: Value::Pick(at),
    })
}

fn custom_without(off: Field) -> Values {
    [
        Field::AutomotiveMemoryStrip,
        Field::AutomotiveUsageBars,
        Field::AutomotiveWhoUses,
        Field::AutomotiveRowBars,
    ]
    .into_iter()
    .fold(preset(2), |values, field| {
        values.with(&flag(field, field != off))
    })
}

pub(in crate::window_view::viewer) fn show(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
    values: Values,
) {
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.set_values(values, window, cx));
    })
    .expect("window");
    settle_all(cx, window, view);
}

fn has(cx: &mut TestAppContext, window: AnyWindowHandle, id: (&'static str, usize)) -> bool {
    cx.update_window(window, |_, window, _| window.try_find(id).is_some())
        .expect("window")
}

fn has_strip(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    cx.update_window(window, |_, window, _| {
        window
            .within(("space-region", 0usize))
            .try_find("memory-strip")
            .is_some()
    })
    .expect("window")
}

#[gpui_kit::test]
fn simple_shows_none_of_the_rich_parts(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    for id in ["space-rich", "space-provenance", "space-panel", "space-who"] {
        assert!(!visible(cx, window, id), "{id}");
    }
    assert!(!visible(cx, window, "elf-noise-bar"));
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Reset_Handler, "));
}

#[gpui_kit::test]
fn rich_shows_the_regions_from_the_mdf_and_sorts_largest_first(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    show(cx, (window, &view), preset(1));
    assert_eq!(
        text_of(cx, window, "space-provenance"),
        "regions from app.mdf · owners from symbols · kinds from section flags"
    );
    let usage = label(cx, window, ("space-usage", 0));
    assert!(
        usage.starts_with("pflash: ") && usage.contains("% used"),
        "{usage}"
    );
    assert!(visible(cx, window, "space-who"));
    assert!(visible(cx, window, "elf-noise-bar"));
    press(cx, window, &view, "ctrl-home");
    settle_all(cx, window, &view);
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Os_Stack, "));
    show(cx, (window, &view), preset(0));
    press(cx, window, &view, "ctrl-home");
    settle_all(cx, window, &view);
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Reset_Handler, "));
}

#[gpui_kit::test]
fn a_file_opened_in_rich_lists_the_largest_symbols_first(cx: &mut TestAppContext) {
    let (window, _) = open_in(cx, demo().expect("the demo builds"), Some(preset(1)));
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Os_Stack, "));
}

#[gpui_kit::test]
fn each_custom_part_turned_off_leaves_its_piece_out(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    show(
        cx,
        (window, &view),
        custom_without(Field::AutomotiveMemoryStrip),
    );
    assert!(!has_strip(cx, window));
    assert!(has(cx, window, ("space-usage", 0)));
    show(
        cx,
        (window, &view),
        custom_without(Field::AutomotiveUsageBars),
    );
    assert!(has_strip(cx, window));
    assert!(!has(cx, window, ("space-usage", 0)));
    assert!(!visible(cx, window, "space-file-bar"));
    show(
        cx,
        (window, &view),
        custom_without(Field::AutomotiveWhoUses),
    );
    assert!(!visible(cx, window, "space-who"));
    assert!(has(cx, window, ("space-usage", 0)));
}

#[gpui_kit::test]
fn picking_a_group_names_it_in_full_in_the_status_and_the_inspector(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    show(cx, (window, &view), preset(1));
    let tile = label(cx, window, ("treemap-tile", 0));
    let name = tile.split(", ").next().expect("a name").to_owned();
    cx.update_window(window, |_, window, cx| {
        window.click(("treemap-tile", 0usize), cx)
    })
    .expect("window");
    settle_all(cx, window, &view);
    assert!(text_of(cx, window, "elf-status").starts_with(&name));
    assert!(
        visible(cx, window, "elf-inspector"),
        "Rich opens the inspector"
    );
    assert_eq!(text_of(cx, window, "elf-inspector-name"), name);
    press(cx, window, &view, "ctrl-i");
    press(cx, window, &view, "ctrl-home");
    settle_all(cx, window, &view);
    assert!(!text_of(cx, window, "elf-status").starts_with(&name));
}

#[gpui_kit::test]
fn the_noise_check_shows_the_hidden_symbols(cx: &mut TestAppContext) {
    let mut spec = firmware(false, false);
    spec.symbols.as_mut().expect("symbols").push(Named {
        bind: 0,
        ..func("local_label", 0x0800_0170, 0, 0)
    });
    let (window, view) = open_bytes(cx, build(&spec).expect("the sample builds"));
    show(cx, (window, &view), preset(1));
    let before = text_of(cx, window, "elf-noise-bar");
    cx.update_window(window, |_, window, cx| window.click("elf-noise", cx))
        .expect("window");
    settle_all(cx, window, &view);
    let after = text_of(cx, window, "elf-noise-bar");
    assert_ne!(before, after);
    assert!(before.starts_with("1 of "), "{before}");
    assert!(after.starts_with("0 of "), "{after}");
}
