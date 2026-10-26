use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Pixels, TestAppContext, VisualTestContext, px, size,
};
use rusty_automotive::elf_sample::{build, firmware, text};

use super::tests::keys;
use crate::window_view::tests::settle;
use crate::window_view::viewer::tests::elf_rich::{open_with_mdf, preset, settle_all};

const REGIONS: u64 = 10;
const STEP: u64 = 0x1_0000;

fn ten_regions() -> (Vec<u8>, &'static str) {
    let mut spec = firmware(false, false);
    spec.parts = (0..REGIONS).map(|at| text(at * STEP, 0x100)).collect();
    spec.loads = (0..REGIONS as usize).map(|at| (at, 5)).collect();
    spec.symbols = None;
    let chips: String = (0..REGIONS)
        .map(|at| {
            format!(
                "chip flash{at} {id} 8 0x{STEP:x} rom\nmap 1 0x{start:x} 0x{STEP:x} {id} 0\n",
                id = at + 1,
                start = at * STEP
            )
        })
        .collect();
    let mdf = format!("space linear 1 8\n{chips}");
    (build(&spec).expect("the sample builds"), mdf.leak())
}

fn bounds(cx: &mut TestAppContext, window: AnyWindowHandle, region: usize) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, _| {
        window
            .within(("space-region", region))
            .find("memory-strip")
            .bounds()
    })
    .expect("window")
}

fn panel(cx: &mut TestAppContext, window: AnyWindowHandle) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, _| window.find("space-top").bounds())
        .expect("window")
}

fn is_shown(strip: Bounds<Pixels>, panel: Bounds<Pixels>) -> bool {
    strip.top() >= panel.top() - px(0.5) && strip.bottom() <= panel.bottom() + px(0.5)
}

#[gpui_kit::test]
fn the_focused_strip_scrolls_into_the_region_panel(cx: &mut TestAppContext) {
    let (bytes, mdf) = ten_regions();
    let (window, view) = open_with_mdf(cx, bytes, mdf, Some(preset(1)));
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(1280.), px(420.)));
    settle(cx, &view);
    settle_all(cx, window, &view);
    let rig = (window, view);
    let last = REGIONS as usize - 1;
    assert!(!is_shown(bounds(cx, window, last), panel(cx, window)));
    keys(cx, &rig, &["tab"]);
    for at in 1..=last {
        keys(cx, &rig, &["down"]);
        assert!(is_shown(bounds(cx, window, at), panel(cx, window)), "{at}");
    }
    for at in (0..last).rev() {
        keys(cx, &rig, &["up"]);
        assert!(is_shown(bounds(cx, window, at), panel(cx, window)), "{at}");
    }
}
