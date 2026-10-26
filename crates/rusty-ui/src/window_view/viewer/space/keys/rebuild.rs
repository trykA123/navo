use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, FocusHandle, Focusable, TestAppContext};
use rusty_automotive::elf_sample::demo;
use rusty_automotive::{Elf, ElfReader, Watch, parse_mdf};

use super::tests::{hints, keys, picked, rich_elf};
use crate::prelude::*;
use crate::test_window::open;
use crate::window_view::viewer::map::tests::SAMPLE;
use crate::window_view::viewer::parts::LayoutParts;
use crate::window_view::viewer::space::model::{RichModel, build_model};
use crate::window_view::viewer::space::rich::{Pick, RichHost, RichSpace};
use crate::window_view::viewer::space::sibling::Siblings;
use crate::window_view::viewer::space::strip::{Label, StripData};
use crate::window_view::viewer::space::{DEMO_MDF, Part};
use crate::window_view::viewer::tests::{label, map_rich, text_of, visible};

const ONE_CHIP: &str = "\
space linear 1 8
chip pflash 1 8 0x10000 rom
map 1 0x8000 0x10000 1 0
";

struct Host {
    rich: RichSpace,
    table: FocusHandle,
}

impl RichHost for Host {
    fn rich(&mut self) -> &mut RichSpace {
        &mut self.rich
    }

    fn space_layout(&self) -> Option<LayoutParts> {
        None
    }

    fn table_focus(&self) -> FocusHandle {
        self.table.clone()
    }

    fn picked(&mut self, _: Pick, _: &mut Context<Self>) {}

    fn regroup(&mut self, _: rusty_automotive::space::Grouping, _: &mut Context<Self>) {}
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(div().id("table").track_focus(&self.table))
            .children(self.rich.strips.clone())
    }
}

fn elf() -> Elf {
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    ElfReader::new(std::io::Cursor::new(demo().expect("the demo builds")))
        .parse(&mut watch)
        .expect("the sample parses")
}

fn model(mdf: &str) -> RichModel {
    let siblings = Siblings {
        mdf: Some(parse_mdf(mdf)),
        ..Siblings::default()
    };
    build_model(&elf(), None, &siblings, Default::default())
}

fn install(cx: &mut TestAppContext, host: &Entity<Host>, mdf: &str) {
    host.update(cx, |host, cx| {
        let data = |region: &std::sync::Arc<rusty_automotive::space::Region>| StripData {
            region: region.clone(),
            owners: None,
            describe: Rc::new(|_| Label {
                name: "x".into(),
                object: None,
            }),
        };
        host.rich.install(model(mdf), data, cx);
    });
    cx.run_until_parked();
}

fn rig(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Host>) {
    let (window, host) = open(cx, |_, cx| Host {
        rich: RichSpace::default(),
        table: cx.focus_handle(),
    });
    install(cx, &host, DEMO_MDF);
    (window, host)
}

fn focus_strip(
    cx: &mut TestAppContext,
    (window, host): &(AnyWindowHandle, Entity<Host>),
    at: usize,
) {
    let handle = host.update(cx, |host, cx| host.rich.strips[at].focus_handle(cx));
    cx.update_window(*window, |_, window, cx| window.focus(&handle, cx))
        .expect("window");
    host.update(cx, |host, _| host.rich.part = Part::Strip(at));
}

fn table_has_focus(
    cx: &mut TestAppContext,
    (window, host): &(AnyWindowHandle, Entity<Host>),
) -> bool {
    let table = host.update(cx, |host, _| host.table.clone());
    cx.update_window(*window, |_, window, _| table.is_focused(window))
        .expect("window")
}

#[gpui_kit::test]
fn a_rebuild_with_fewer_regions_moves_a_lost_focus_to_the_table(cx: &mut TestAppContext) {
    let rig = rig(cx);
    focus_strip(cx, &rig, 1);
    install(cx, &rig.1, ONE_CHIP);
    assert_eq!(rig.1.update(cx, |host, _| host.rich.strips.len()), 1);
    assert!(table_has_focus(cx, &rig));
    assert_eq!(rig.1.update(cx, |host, _| host.rich.part), Part::Table);
}

#[gpui_kit::test]
fn a_rebuild_keeps_the_focus_of_a_strip_that_stays(cx: &mut TestAppContext) {
    let rig = rig(cx);
    focus_strip(cx, &rig, 0);
    install(cx, &rig.1, ONE_CHIP);
    assert!(!table_has_focus(cx, &rig));
    let handle = rig
        .1
        .update(cx, |host, cx| host.rich.strips[0].focus_handle(cx));
    let focused = cx
        .update_window(rig.0, |_, window, _| handle.is_focused(window))
        .expect("window");
    assert!(focused);
}

#[gpui_kit::test]
fn esc_clears_a_picked_group_and_the_inspector_stays_until_the_next_esc(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    let name = label(cx, window, ("treemap-tile", 0));
    let name = name.split(", ").next().expect("a name").to_owned();
    cx.update_window(window, |_, window, cx| {
        window.click(("treemap-tile", 0usize), cx)
    })
    .expect("window");
    keys(cx, &rig, &[]);
    assert_eq!(picked(cx, window), name);
    keys(cx, &rig, &["escape"]);
    assert!(!text_of(cx, window, "elf-status").starts_with(&name));
    assert!(visible(cx, window, "elf-inspector"));
    keys(cx, &rig, &["escape"]);
    assert!(!visible(cx, window, "elf-inspector"));
    assert!(visible(cx, window, "elf-status"));
}

#[gpui_kit::test]
fn esc_clears_a_picked_block_in_the_map_view(cx: &mut TestAppContext) {
    let (window, view) = map_rich::open_in(cx, SAMPLE, (None, Some(map_rich::preset(1))));
    let rig = (window, view);
    keys(cx, &rig, &["tab", "right"]);
    let name = text_of(cx, window, "map-inspector-name");
    assert!(text_of(cx, window, "map-status").starts_with(&name));
    keys(cx, &rig, &["escape"]);
    assert!(!text_of(cx, window, "map-status").starts_with(&name));
    assert!(visible(cx, window, "map-status"));
}

#[gpui_kit::test]
fn alt_g_keeps_the_zoom_and_the_caret_of_a_strip(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["tab", "right", "enter"]);
    assert!(hints(cx, window).contains("Unzoom"));
    assert_eq!(picked(cx, window), ".text");
    keys(cx, &rig, &["alt-g"]);
    assert!(hints(cx, window).contains("Unzoom"));
    assert_eq!(picked(cx, window), ".text");
    keys(cx, &rig, &["backspace"]);
    assert!(!hints(cx, window).contains("Unzoom"));
}

#[gpui_kit::test]
fn alt_g_in_the_treemap_keeps_the_keys_on_the_treemap(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["tab", "tab", "tab"]);
    assert!(hints(cx, window).contains("Select"));
    keys(cx, &rig, &["alt-g"]);
    let first = label(cx, window, ("treemap-tile", 0));
    let name = first.split(", ").next().expect("a name").to_owned();
    keys(cx, &rig, &["enter"]);
    assert_eq!(picked(cx, window), name);
}
