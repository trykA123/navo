use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, IntoElement, Modifiers, Pixels, Render, Role, TestAppContext,
    Window, px,
};

use super::{FKeyBar, KeyLayer};
use crate::test_window::{open, open_sized};
use crate::window_view::{KEY_WARNING, key_labels, layer_keys};

struct Keys;

impl Render for Keys {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        FKeyBar::new("fkeys", [("F3", "View"), ("F5", "Copy"), ("F8", "Delete")])
    }
}

#[gpui_kit::test]
fn the_bar_shows_each_key_with_its_label(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Keys);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("fkeys").role(), Some(Role::Group));
        let keys = window.within("fkeys");
        let labels: Vec<Option<String>> = (0..3usize)
            .map(|index| keys.find(("fkey", index)).label().map(str::to_owned))
            .collect();
        assert_eq!(
            labels,
            [Some("F3 View"), Some("F5 Copy"), Some("F8 Delete")].map(|l| l.map(str::to_owned))
        );
        assert!(keys.try_find(("fkey", 3usize)).is_none());
        assert!(keys.try_find(("fkey-mod", 0usize)).is_none());
        assert!(keys.try_find("fkey-pill-slot").is_none());
    })
    .expect("window");
}

const LABELS: [[&str; 4]; 3] = [
    ["", "", "", "Left drive"],
    ["View", "", "By name", ""],
    ["Delete", "Delete for good", "", ""],
];

fn keys_of(held: Option<KeyLayer>) -> Vec<(&'static str, &'static str)> {
    let layer = match held {
        None => 0,
        Some(KeyLayer::Shift) => 1,
        Some(KeyLayer::Ctrl) => 2,
        Some(KeyLayer::Alt) => 3,
    };
    ["F1", "F3", "F8"]
        .into_iter()
        .zip(LABELS)
        .map(|(key, labels)| (key, labels[layer]))
        .collect()
}

struct Layered(Option<KeyLayer>);

impl Render for Layered {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        FKeyBar::new("fkeys", keys_of(self.0))
            .layers(self.0, LABELS)
            .reserve_pill()
    }
}

struct Shown {
    labels: Vec<String>,
    lit: Vec<Option<bool>>,
    places: Vec<Bounds<Pixels>>,
    slot: Bounds<Pixels>,
}

fn shown(cx: &mut TestAppContext, window: AnyWindowHandle) -> Shown {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let keys = window.within("fkeys");
        let found: Vec<_> = (0..3usize).map(|i| keys.find(("fkey", i))).collect();
        Shown {
            labels: found
                .iter()
                .map(|key| key.label().unwrap_or_default().to_owned())
                .collect(),
            lit: (0..3usize)
                .map(|i| keys.find(("fkey-mod", i)).selected())
                .collect(),
            places: found.iter().map(|key| key.bounds()).collect(),
            slot: keys.find("fkey-pill-slot").bounds(),
        }
    })
    .expect("window")
}

#[gpui_kit::test]
fn holding_a_modifier_lights_only_its_cap(cx: &mut TestAppContext) {
    let cases = [
        (None, [false, false, false]),
        (Some(KeyLayer::Shift), [true, false, false]),
        (Some(KeyLayer::Ctrl), [false, true, false]),
        (Some(KeyLayer::Alt), [false, false, true]),
    ];
    for (held, lit) in cases {
        let (window, _) = open(cx, move |_, _| Layered(held));
        assert_eq!(shown(cx, window).lit, lit.map(Some), "{held:?}");
    }
}

#[gpui_kit::test]
fn a_held_layer_shows_its_labels_and_dims_keys_without_one(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Layered(None));
    let base = shown(cx, window);
    view.update(cx, |view, cx| {
        view.0 = Some(KeyLayer::Shift);
        cx.notify();
    });
    let shift = shown(cx, window);
    assert_eq!(base.labels, ["F1", "F3 View", "F8 Delete"]);
    assert_eq!(shift.labels, ["F1", "F3", "F8 Delete for good"]);
    assert_eq!(base.places, shift.places);
}

struct Warned;

impl Render for Warned {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        FKeyBar::new("fkeys", [("F7", ""), ("F8", "Delete")])
            .layers(
                Some(KeyLayer::Shift),
                [["", "", "", ""], ["Delete", "Delete", "", ""]],
            )
            .warn(KeyLayer::Shift, 1, "Delete for good")
    }
}

#[gpui_kit::test]
fn a_warned_key_speaks_its_full_meaning(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Warned);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let keys = window.within("fkeys");
        assert_eq!(keys.find(("fkey", 0usize)).label(), Some("F7"));
        assert_eq!(
            keys.find(("fkey", 1usize)).label(),
            Some("F8 Delete for good")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_pill_slot_stays_empty_right_of_the_last_key(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Layered(None));
    let bar = shown(cx, window);
    let last = bar.places.last().copied().expect("keys");
    assert!(bar.slot.left() >= last.right());
    assert!(bar.slot.size.width > gpui_kit::px(0.));
}

#[test]
fn only_one_held_modifier_picks_a_layer() {
    let held = |shift, control, alt| {
        KeyLayer::held(&Modifiers {
            shift,
            control,
            alt,
            ..Modifiers::default()
        })
    };
    assert_eq!(held(false, false, false), None);
    assert_eq!(held(true, false, false), Some(KeyLayer::Shift));
    assert_eq!(held(false, true, false), Some(KeyLayer::Ctrl));
    assert_eq!(held(false, false, true), Some(KeyLayer::Alt));
    assert_eq!(held(false, true, true), None);
    assert_eq!(held(true, true, false), None);
}

struct MainBar(Option<KeyLayer>);

impl Render for MainBar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (layer, index, spoken) = KEY_WARNING;
        FKeyBar::new("fkeys", layer_keys(self.0))
            .layers(self.0, key_labels())
            .warn(layer, index, spoken)
            .reserve_pill()
    }
}

fn main_bar_bounds(
    cx: &mut TestAppContext,
    width: Pixels,
    held: Option<KeyLayer>,
) -> Vec<Bounds<Pixels>> {
    let (window, _) = open_sized(cx, width, move |_, _| MainBar(held));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within("fkeys");
        let keys = (0..10usize).map(|i| bar.find(("fkey", i)).bounds());
        let mods = (0..3usize).map(|i| bar.find(("fkey-mod", i)).bounds());
        keys.chain(mods)
            .chain([bar.find("fkey-pill-slot").bounds()])
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_main_bar_fits_at_1100_and_1280_px_in_every_layer(cx: &mut TestAppContext) {
    let layers = [
        None,
        Some(KeyLayer::Shift),
        Some(KeyLayer::Ctrl),
        Some(KeyLayer::Alt),
    ];
    for width in [px(1100.), px(1280.)] {
        for held in layers {
            for part in main_bar_bounds(cx, width, held) {
                assert!(part.left() >= px(0.), "{width:?} {held:?}: {part:?}");
                assert!(part.right() <= width, "{width:?} {held:?}: {part:?}");
            }
        }
    }
}

#[gpui_kit::test]
fn the_main_bar_keeps_every_key_in_place_across_layers(cx: &mut TestAppContext) {
    for width in [px(1100.), px(1280.)] {
        let base: Vec<Pixels> = main_bar_bounds(cx, width, None)[..10]
            .iter()
            .map(|key| key.left())
            .collect();
        for held in KeyLayer::ALL.map(Some) {
            let shown: Vec<Pixels> = main_bar_bounds(cx, width, held)[..10]
                .iter()
                .map(|key| key.left())
                .collect();
            assert_eq!(shown, base, "{width:?} {held:?}");
        }
    }
}
