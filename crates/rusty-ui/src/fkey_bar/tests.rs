use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, IntoElement, Modifiers, Pixels, Render, Role, TestAppContext,
    Window,
};

use super::{FKeyBar, KeyLayer};
use crate::test_window::open;

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
fn a_held_layer_shows_its_labels_in_the_same_places(cx: &mut TestAppContext) {
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
