use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AnyWindowHandle, Entity, Subscription, TestAppContext};
use rusty_automotive::space::{
    Block, Kind, NameRef, Owner, OwnerRef, Owners, Region, RegionSource, ScaleMode,
};

use super::cache::{Item, Key, Layout, Over, Step, is_dim};
use super::{Label, MemoryStrip, StripData, StripEvent};
use crate::prelude::*;
use crate::test_pointer::{dispatch, down, moved, up};
use crate::test_window::open;

const BASE: u64 = 0x8000_0000;
const TINY: usize = 40;
const WIDTH: f32 = 800.;

fn block(start: u64, size: u64, kind: Kind, at: usize) -> Block {
    Block {
        start: BASE + start,
        size,
        kind,
        owner: OwnerRef::ElfSection(at),
    }
}

fn region() -> Region {
    let mut blocks = vec![
        block(0, 0x4000, Kind::Code, 0),
        block(0x4000, 0x2000, Kind::Rodata, 1),
    ];
    for at in 0..TINY {
        blocks.push(block(0x6000 + at as u64 * 4, 4, Kind::Data, 2 + at));
    }
    blocks.push(block(0x8000, 0x2000, Kind::Bss, 2 + TINY));
    Region {
        name: "pflash".into(),
        start: Some(BASE),
        start_estimated: false,
        total: Some(0x1_0000),
        chip: None,
        reserved: None,
        used: 0xA000,
        by_kind: [0; Kind::COUNT],
        blocks,
        source: RegionSource::Mdf,
    }
}

fn owners() -> Owners {
    let owner = |start: u64, at: u32| Owner {
        start: BASE + start,
        size: 0x2000,
        name: NameRef::Symbol(at),
        kind: Kind::Code,
        section: 0,
        object: None,
        source_file: None,
        estimated: false,
        image: false,
    };
    let mut owners = Owners::default();
    owners.items = vec![owner(0, 0), owner(0x2000, 1)];
    owners
}

fn true_scale(zoom: Option<usize>) -> Key {
    Key {
        width: 800,
        mode: ScaleMode::True,
        zoom,
    }
}

fn last() -> usize {
    region().blocks.len() - 1
}

#[test]
fn a_column_maps_to_the_block_under_it() {
    let region = region();
    let layout = Layout::build(&region, None, true_scale(None));
    let hit = layout.hit(&region, 100).expect("hit");
    assert_eq!((hit.block, hit.over), (0, Over::One(Item::Block(0))));
    let hit = layout.hit(&region, 250).expect("hit");
    assert_eq!((hit.block, hit.over), (1, Over::One(Item::Block(1))));
    assert!(
        layout.hit(&region, 790).is_none(),
        "free space has no block"
    );
}

#[test]
fn a_merged_column_names_its_sections_and_picks_one_of_them() {
    let region = region();
    let layout = Layout::build(&region, None, true_scale(None));
    let column = (layout.scale.x(BASE + 0x6000 + 2) * 800.) as u32;
    let hit = layout.hit(&region, column).expect("hit");
    let Over::Sections { count, bytes } = hit.over else {
        panic!("expected a merged cell, got {:?}", hit.over);
    };
    assert!(count > 1);
    assert_eq!(bytes, count as u64 * 4);
    assert!((2..2 + TINY).contains(&hit.block), "block {}", hit.block);
}

#[test]
fn a_wide_block_with_owners_hits_the_owner_under_the_pointer() {
    let (region, owners) = (region(), owners());
    let layout = Layout::build(&region, Some(&owners), true_scale(None));
    let hit = layout.hit(&region, 50).expect("hit");
    assert_eq!((hit.block, hit.over), (0, Over::One(Item::Owner(0))));
    let hit = layout.hit(&region, 150).expect("hit");
    assert_eq!((hit.block, hit.over), (0, Over::One(Item::Owner(1))));
}

#[test]
fn the_caret_spans_the_cell_of_its_block() {
    let region = region();
    let layout = Layout::build(&region, None, true_scale(None));
    let span = layout.span_of(1).expect("span");
    assert!(span.contains(&250) && span.start >= 199 && span.end <= 301);
    let zoomed = Layout::build(&region, None, true_scale(Some(0)));
    assert!(zoomed.span_of(1).is_none(), "block 1 is outside the zoom");
    assert_eq!(zoomed.span_of(0), Some(0..800));
}

#[test]
fn a_cell_is_dim_only_when_none_of_its_blocks_is_lit() {
    let lit = [false, true, false];
    assert!(!is_dim(None, &(0..1)));
    assert!(is_dim(Some(&lit), &(0..1)));
    assert!(!is_dim(Some(&lit), &(0..3)));
    assert!(is_dim(Some(&lit), &(5..6)), "a block past the set is dim");
}

#[test]
fn steps_move_through_blocks_and_stop_at_the_ends() {
    let region = region();
    let layout = Layout::build(&region, None, true_scale(None));
    assert_eq!(layout.step(None, Step::Next), Some(0));
    assert_eq!(layout.step(None, Step::Prev), Some(last()));
    assert_eq!(layout.step(Some(0), Step::Prev), Some(0));
    assert_eq!(layout.step(Some(0), Step::Next), Some(1));
    assert_eq!(layout.step(Some(1), Step::Last), Some(last()));
    assert_eq!(layout.step(Some(last()), Step::Next), Some(last()));
    assert_eq!(layout.step(Some(last()), Step::First), Some(0));
}

#[test]
fn steps_stay_inside_a_zoomed_block() {
    let region = region();
    let zoomed = Layout::build(&region, None, true_scale(Some(1)));
    assert_eq!(zoomed.visible(), 1..2);
    assert_eq!(zoomed.step(Some(1), Step::Next), Some(1));
    assert_eq!(zoomed.step(Some(1), Step::Prev), Some(1));
    assert_eq!(zoomed.step(Some(5), Step::Next), Some(1));
}

#[test]
fn widths_share_a_key_within_eight_pixels() {
    let key = |width| Key::new(width, ScaleMode::Compressed, None).map(|key| key.width);
    assert_eq!(key(803.), Some(800));
    assert_eq!(key(807.9), Some(800));
    assert_eq!(key(808.), Some(808));
    assert_eq!(key(7.), None);
    assert_eq!(key(f32::NAN), None);
}

#[test]
fn compressed_scale_marks_the_free_tail_as_a_break() {
    let region = region();
    let key = Key {
        mode: ScaleMode::Compressed,
        ..true_scale(None)
    };
    let layout = Layout::build(&region, None, key);
    assert_eq!(
        layout.scale.breaks().len(),
        2,
        "the gap before bss and the free tail"
    );
    let end = layout.cells.last().expect("cells").x1;
    assert!(end > 760, "used blocks fill most of the strip, end {end}");
}

struct Host {
    strip: Entity<MemoryStrip>,
    events: Rc<RefCell<Vec<StripEvent>>>,
    _events: Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(WIDTH)).child(self.strip.clone())
    }
}

fn data() -> StripData {
    let names = |item| match item {
        Item::Block(at) => format!("section_{at}"),
        Item::Owner(at) => format!("symbol_{at}"),
    };
    StripData {
        region: Arc::new(region()),
        owners: Some(Arc::new(owners())),
        describe: Rc::new(move |item| Label {
            name: names(item).into(),
            object: None,
        }),
    }
}

fn host(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Host>) {
    let (window, host) = open(cx, |_, cx| {
        let strip = cx.new(|cx| {
            let mut strip = MemoryStrip::new(data(), px(28.), cx);
            strip.mode = ScaleMode::True;
            strip
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let subscription = cx.subscribe(&strip, move |_, event: &StripEvent, _| {
            seen.borrow_mut().push(*event);
        });
        Host {
            strip,
            events,
            _events: subscription,
        }
    });
    frame(window, cx);
    frame(window, cx);
    (window, host)
}

fn frame(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
}

fn strip(host: &Entity<Host>, cx: &mut TestAppContext) -> Entity<MemoryStrip> {
    host.read_with(cx, |host, _| host.strip.clone())
}

fn events(host: &Entity<Host>, cx: &mut TestAppContext) -> Vec<StripEvent> {
    host.read_with(cx, |host, _| host.events.borrow().clone())
}

fn layout_ptr(strip: &Entity<MemoryStrip>, cx: &mut TestAppContext) -> Option<*const Layout> {
    strip.read_with(cx, |strip, _| strip.layout.as_ref().map(Rc::as_ptr))
}

#[gpui_kit::test]
fn the_strip_lays_out_at_its_measured_width(cx: &mut TestAppContext) {
    let (_, host) = host(cx);
    let strip = strip(&host, cx);
    let key = strip.read_with(cx, |strip, _| strip.layout.as_ref().map(|l| l.key));
    assert_eq!(key, Some(true_scale(None)));
}

#[gpui_kit::test]
async fn hovering_shows_the_block_under_the_pointer(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let at = point(px(250.), px(10.));
    dispatch(cx, window, moved(None, at, Modifiers::default()));
    dispatch(cx, window, moved(None, at, Modifiers::default()));
    assert_eq!(events(&host, cx), vec![StripEvent::Hover(Some(1))]);
    cx.wait_for(window, Duration::from_secs(2), |window, _| {
        window
            .try_find("tooltip")
            .and_then(|tip| tip.label().map(str::to_owned))
            .is_some_and(|label| label.starts_with("section_1 · read-only data · 8.0 KB"))
    })
    .await;
}

#[gpui_kit::test]
async fn hovering_an_inner_cell_names_the_symbol(cx: &mut TestAppContext) {
    let (window, _) = host(cx);
    let at = point(px(150.), px(10.));
    dispatch(cx, window, moved(None, at, Modifiers::default()));
    dispatch(cx, window, moved(None, at, Modifiers::default()));
    cx.wait_for(window, Duration::from_secs(2), |window, _| {
        window
            .try_find("tooltip")
            .and_then(|tip| tip.label().map(str::to_owned))
            .is_some_and(|label| label.starts_with("symbol_1 · code"))
    })
    .await;
}

#[gpui_kit::test]
fn a_click_picks_the_block_and_focuses_the_strip(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let strip = strip(&host, cx);
    let at = point(px(250.), px(10.));
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    dispatch(cx, window, up(MouseButton::Left, at, Modifiers::default()));
    assert!(events(&host, cx).contains(&StripEvent::Pick(1)));
    assert_eq!(strip.read_with(cx, |strip, _| strip.caret()), Some(1));
    let focused = cx
        .update_window(window, |_, window, cx| {
            strip.read(cx).focus.is_focused(window)
        })
        .expect("window");
    assert!(focused);
}

#[gpui_kit::test]
fn a_click_on_free_space_focuses_without_a_pick(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let strip = strip(&host, cx);
    let at = point(px(790.), px(10.));
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    assert!(events(&host, cx).is_empty());
    assert_eq!(strip.read_with(cx, |strip, _| strip.caret()), None);
}

#[gpui_kit::test]
fn steps_pick_blocks_for_the_inspector(cx: &mut TestAppContext) {
    let (_, host) = host(cx);
    let strip = strip(&host, cx);
    let moved = strip.update(cx, |strip, cx| {
        [Step::Next, Step::Next, Step::Prev, Step::Last].map(|step| strip.step(step, cx))
    });
    assert_eq!(moved, [true; 4]);
    let picks = [0, 1, 0, last()].map(StripEvent::Pick).to_vec();
    assert_eq!(events(&host, cx), picks);
}

#[gpui_kit::test]
fn zoom_needs_a_caret_and_zoom_out_only_acts_when_zoomed(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let strip = strip(&host, cx);
    assert!(!strip.update(cx, |strip, cx| strip.zoom_in(cx)));
    assert!(!strip.update(cx, |strip, cx| strip.zoom_out(cx)));
    strip.update(cx, |strip, cx| strip.set_caret(Some(1), cx));
    assert!(strip.update(cx, |strip, cx| strip.zoom_in(cx)));
    assert!(!strip.update(cx, |strip, cx| strip.zoom_in(cx)));
    frame(window, cx);
    let span = strip.read_with(cx, |strip, _| {
        strip.layout.as_ref().and_then(|l| l.span_of(1))
    });
    assert_eq!(span, Some(0..800));
    assert!(strip.update(cx, |strip, cx| strip.zoom_out(cx)));
    assert_eq!(strip.read_with(cx, |strip, _| strip.zoom()), None);
    assert!(!strip.update(cx, |strip, cx| strip.zoom_out(cx)));
}

#[gpui_kit::test]
fn the_cache_survives_selection_changes(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let strip = strip(&host, cx);
    let before = layout_ptr(&strip, cx);
    assert!(before.is_some());
    strip.update(cx, |strip, cx| {
        strip.set_caret(Some(3), cx);
        strip.set_lit(Some(Rc::from(vec![true, false])), cx);
        strip.step(Step::Next, cx);
    });
    frame(window, cx);
    assert_eq!(layout_ptr(&strip, cx), before);
    strip.update(cx, |strip, cx| strip.set_mode(ScaleMode::Compressed, cx));
    frame(window, cx);
    assert_ne!(layout_ptr(&strip, cx), before);
}
