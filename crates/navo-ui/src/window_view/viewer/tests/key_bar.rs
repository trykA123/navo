use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Bounds, Pixels, TestAppContext, px};

use super::{Log, Setup, hooks, mixed, options_with, redraw};
use crate::window_view::tests::{fake, open_options_sized, press};
use crate::window_view::viewer::Kinds;

struct Bar {
    bar: Bounds<Pixels>,
    keys: Vec<(String, Bounds<Pixels>)>,
}

fn bar_after(cx: &mut TestAppContext, (setup, keys): (Setup, &[&str]), width: Pixels) -> Bar {
    let options = options_with(fake(), hooks(setup, &Log::default()), Kinds::builtin());
    let (window, view) = open_options_sized(cx, options, width);
    for key in ["down", "f3"].iter().chain(keys) {
        press(cx, window, &view, key);
    }
    redraw(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.find("fkeys").bounds();
        let found = window.within("fkeys");
        Bar {
            bar,
            keys: (0usize..24)
                .filter_map(|index| {
                    let key = found.try_find(("fkey", index))?;
                    Some((key.label()?.to_owned(), key.bounds()))
                })
                .collect(),
        }
    })
    .expect("window")
}

fn truncated(narrow: &Bar, natural: &Bar) -> Vec<String> {
    narrow
        .keys
        .iter()
        .zip(&natural.keys)
        .filter(|((_, fitted), (_, whole))| fitted.size.width < whole.size.width)
        .map(|((label, _), _)| label.clone())
        .collect()
}

fn assert_whole_at_1280(
    cx: &mut TestAppContext,
    opened: impl Fn() -> (Setup, &'static [&'static str]),
) {
    let natural = bar_after(cx, opened(), px(2560.));
    let narrow = bar_after(cx, opened(), px(1280.));
    let last = narrow.keys.last().expect("keys");
    assert_eq!(last.0, "Esc Close");
    assert!(last.1.right() <= narrow.bar.right(), "{last:?}");
    assert_eq!(truncated(&narrow, &natural), Vec::<String>::new());
}

#[gpui_kit::test]
fn esc_close_and_every_other_key_stay_whole_on_the_paged_text_bar_at_1280_px(
    cx: &mut TestAppContext,
) {
    assert_whole_at_1280(cx, || (Setup::new(mixed(4096)), &["alt-1"]));
}

#[gpui_kit::test]
fn esc_close_and_every_other_key_stay_whole_on_the_read_only_text_bar_at_1280_px(
    cx: &mut TestAppContext,
) {
    assert_whole_at_1280(cx, || (Setup::new(b"hello\nworld\n".to_vec()), &[]));
}
