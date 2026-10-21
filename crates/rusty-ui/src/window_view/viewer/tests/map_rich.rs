use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_automotive::elf_sample::{Part, Spec, bss, build, data, text};

use super::structured::fake_with;
use super::{Log, Setup, hooks, label, press_only, redraw, text_of, visible, window_with};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::map::tests::SAMPLE;
use crate::window_view::viewer::{FileHooks, Head, Kinds};

fn rodata_at(name: &'static str, address: u64, len: usize) -> Part {
    Part {
        name,
        flags: 2,
        ..data(address, len)
    }
}

fn sibling_elf() -> Vec<u8> {
    build(&Spec {
        is_64: false,
        is_big: false,
        machine: 36,
        entry: 0,
        parts: vec![
            Part {
                name: ".text.boot",
                ..text(0x8000_0000, 0x180)
            },
            rodata_at(".data.cal", 0x8000_1000, 0x40),
            Part {
                name: ".bss.ram",
                ..bss(0x7000_0000, 0x400)
            },
        ],
        symbols: None,
        loads: Vec::new(),
        debug: Vec::new(),
    })
    .expect("the sample builds")
}

fn with_elf(files: FileHooks, elf: Option<Vec<u8>>) -> FileHooks {
    let (head, bytes) = (files.head.clone(), files.bytes.clone());
    let (head_elf, bytes_elf) = (elf.clone(), elf);
    FileHooks {
        head: Arc::new(move |location| {
            if location.as_str().ends_with(".elf") {
                return head_elf
                    .as_ref()
                    .map(|elf| Head {
                        bytes: elf.iter().copied().take(8192).collect(),
                        len: elf.len() as u64,
                    })
                    .ok_or_else(|| "not found".to_owned());
            }
            if location.as_str().ends_with(".map") {
                return head(location);
            }
            Err("not found".to_owned())
        }),
        bytes: Arc::new(move |location, limit| {
            if location.as_str().ends_with(".elf") {
                return bytes_elf.clone().ok_or_else(|| "not found".to_owned());
            }
            bytes(location, limit)
        }),
        ..files
    }
}

fn settle_all(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, view);
        redraw(cx, window);
    }
}

fn open(
    cx: &mut TestAppContext,
    text: &str,
    elf: Option<Vec<u8>>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_in(cx, text, (elf, None))
}

fn open_in(
    cx: &mut TestAppContext,
    text: &str,
    (elf, values): (Option<Vec<u8>>, Option<Values>),
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let bytes = text.as_bytes().to_vec();
    let len = bytes.len() as u64;
    let files = with_elf(hooks(Setup::new(bytes), &Log::default()), elf);
    let (window, view) = window_with(cx, fake_with("app.map", len), files, Kinds::builtin());
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

fn preset(at: usize) -> Values {
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

fn show(
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
    let (window, _) = open(cx, SAMPLE, None);
    for id in ["space-rich", "space-provenance", "space-panel", "space-who"] {
        assert!(!visible(cx, window, id), "{id}");
    }
    assert!(label(cx, window, ("map-row", 0)).starts_with("mpe:dsram0, "));
}

#[gpui_kit::test]
fn rich_shows_the_regions_from_the_map_and_lists_the_largest_sections_first(
    cx: &mut TestAppContext,
) {
    let (window, view) = open(cx, SAMPLE, None);
    show(cx, (window, &view), preset(1));
    assert_eq!(
        text_of(cx, window, "space-provenance"),
        "regions from app.map · owners from app.map · kinds from section names"
    );
    let usage = label(cx, window, ("space-usage", 0));
    assert!(
        usage.starts_with("mpe:flash0: ") && usage.contains("% used"),
        "{usage}"
    );
    assert!(visible(cx, window, "space-who"));
    assert!(label(cx, window, ("map-row", 0)).starts_with(".bss.ram (4), "));
    show(cx, (window, &view), preset(0));
    assert!(label(cx, window, ("map-row", 0)).starts_with("mpe:dsram0, "));
}

#[gpui_kit::test]
fn a_file_opened_in_rich_starts_on_the_largest_section(cx: &mut TestAppContext) {
    let (window, _) = open_in(cx, SAMPLE, (None, Some(preset(1))));
    assert!(label(cx, window, ("map-row", 0)).starts_with(".bss.ram (4), "));
    assert!(text_of(cx, window, "map-cursor").starts_with(".bss.ram (4)"));
}

#[gpui_kit::test]
fn a_sibling_elf_gives_the_kinds_and_the_provenance_says_so(cx: &mut TestAppContext) {
    let (window, view) = open(cx, SAMPLE, Some(sibling_elf()));
    show(cx, (window, &view), preset(1));
    assert_eq!(
        text_of(cx, window, "space-provenance"),
        "regions from app.map · owners from app.map · kinds from app.elf"
    );
}

#[gpui_kit::test]
fn a_sibling_that_is_not_an_elf_is_named_as_skipped(cx: &mut TestAppContext) {
    let (window, view) = open(cx, SAMPLE, Some(b"plain text".to_vec()));
    show(cx, (window, &view), preset(1));
    assert_eq!(
        text_of(cx, window, "space-provenance"),
        "regions from app.map · owners from app.map · kinds from section names · app.elf skipped: format not recognised"
    );
}

#[gpui_kit::test]
fn reserved_bytes_are_taken_out_of_the_free_figure(cx: &mut TestAppContext) {
    let reserved = SAMPLE.replace(
        "| mpe:flash0 | 896 | 64 | 0 | 64576 | 65536 |",
        "| mpe:flash0 | 896 | 64 | 1024 | 63552 | 65536 |",
    );
    let (window, view) = open(cx, &reserved, None);
    show(cx, (window, &view), preset(1));
    let free = crate::format::size(65536 - 896 - 1024);
    let usage = label(cx, window, ("space-usage", 0));
    assert!(usage.ends_with(&format!("{free} free")), "{usage}");
}

#[gpui_kit::test]
fn each_custom_part_turned_off_leaves_its_piece_out(cx: &mut TestAppContext) {
    let (window, view) = open(cx, SAMPLE, None);
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
    let (window, view) = open(cx, SAMPLE, None);
    show(cx, (window, &view), preset(1));
    let tile = label(cx, window, ("treemap-tile", 0));
    let name = tile.split(", ").next().expect("a name").to_owned();
    cx.update_window(window, |_, window, cx| {
        window.click(("treemap-tile", 0usize), cx)
    })
    .expect("window");
    settle_all(cx, window, &view);
    assert!(text_of(cx, window, "map-status").starts_with(&name));
    assert!(
        visible(cx, window, "map-inspector"),
        "Rich opens the inspector"
    );
    assert_eq!(text_of(cx, window, "map-inspector-name"), name);
    press(cx, window, &view, "ctrl-i");
    press(cx, window, &view, "down");
    settle_all(cx, window, &view);
    assert!(!text_of(cx, window, "map-status").starts_with(&name));
}
