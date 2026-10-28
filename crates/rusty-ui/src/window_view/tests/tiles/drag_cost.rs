use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Entity, InputEvent as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, TestAppContext, VisualTestContext, Window, point,
    px, size,
};
use rusty_fs::FakeFs;

use super::super::{open_options_sized, options_for, two_panes};
use crate::window_view::FerriteWindow;

const ENTRIES: usize = 5_000;
const MOVES: usize = 60;
const FRAME: Duration = Duration::from_millis(16);

fn big_folder() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for index in 0..ENTRIES {
        let name = format!(
            "home/quarterly review of the northern test benches and calibration runs {index:05}.txt"
        );
        fs.add_file(&name, index as u64 * 37);
    }
    Arc::new(fs)
}

fn with_bundled_fonts(cx: &TestAppContext) -> TestAppContext {
    let platform = gpui_kit::platform::current_platform(true);
    let fonts =
        TestAppContext::build_with_text_system(cx.dispatcher.clone(), None, platform.text_system());
    fonts.update(|cx| crate::assets::load_fonts(cx).expect("bundled fonts load"));
    fonts
}

fn open_big(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = big_folder();
    let state = two_panes(&fs);
    let opened = open_options_sized(cx, options_for(fs, state), px(1920.));
    VisualTestContext::from_window(opened.0, cx).simulate_resize(size(px(1920.), px(1080.)));
    cx.run_until_parked();
    opened
}

fn press_divider(window: &mut Window, cx: &mut App) -> Point<Pixels> {
    window.render_frame(cx);
    let from = window.find("divider-root").bounds().center();
    let down = MouseDownEvent {
        button: MouseButton::Left,
        position: from,
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    };
    window.dispatch_event(down.to_platform_input(), cx);
    window.draw(cx).clear(cx);
    from
}

fn drag_frames(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<Duration> {
    cx.update_window(window, |_, window, cx| {
        let from = press_divider(window, cx);
        let area = window.find("tiles").bounds();
        let mut frames = Vec::with_capacity(MOVES);
        let mut to = from;
        for step in 0..MOVES {
            let share = 0.3 + 0.4 * step as f32 / MOVES as f32;
            to = point(area.left() + area.size.width * share, from.y);
            let started = Instant::now();
            let moved = MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            };
            window.dispatch_event(moved.to_platform_input(), cx);
            window.draw(cx).clear(cx);
            frames.push(started.elapsed());
        }
        let up = MouseUpEvent {
            button: MouseButton::Left,
            position: to,
            modifiers: Default::default(),
            click_count: 1,
        };
        window.dispatch_event(up.to_platform_input(), cx);
        frames
    })
    .expect("window")
}

fn idle_frames(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
) -> Vec<Duration> {
    (0..MOVES)
        .map(|_| {
            view.update(cx, |_, cx| cx.notify());
            cx.update_window(window, |_, window, cx| {
                let started = Instant::now();
                window.draw(cx).clear(cx);
                started.elapsed()
            })
            .expect("window")
        })
        .collect()
}

#[allow(
    clippy::print_stderr,
    reason = "the timing test prints its frame times"
)]
fn p95(label: &str, frames: &mut [Duration]) -> Duration {
    frames.sort();
    let at = |share: f32| frames[((frames.len() - 1) as f32 * share) as usize];
    eprintln!(
        "{label}: {} frames, median {:?}, p95 {:?}, max {:?}",
        frames.len(),
        at(0.5),
        at(0.95),
        at(1.)
    );
    at(0.95)
}

/// `cargo nextest run -p rusty-ui --release --run-ignored only divider_drag_frame_cost --no-capture`
#[gpui_kit::test]
#[ignore = "timing: run in release by hand"]
fn divider_drag_frame_cost_with_two_panes_of_5000_files(cx: &mut TestAppContext) {
    let mut fonts = with_bundled_fonts(cx);
    let (window, view) = open_big(&mut fonts);
    let idle = p95("idle frame", &mut idle_frames(&mut fonts, (window, &view)));
    let drag = p95("divider drag", &mut drag_frames(&mut fonts, window));
    assert!(drag <= FRAME, "p95 drag frame {drag:?} > {FRAME:?}");
    assert!(
        drag <= idle * 11 / 10,
        "p95 drag frame {drag:?} costs more than an idle frame {idle:?}"
    );
}
