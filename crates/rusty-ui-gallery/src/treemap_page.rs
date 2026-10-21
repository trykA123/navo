use std::rc::Rc;

use rusty_automotive::space::{Item, Part, Rect, layout};
use rusty_ui::prelude::*;
use rusty_ui::tokens;
use rusty_ui::window_view::{Treemap, TreemapEvent, TreemapGroup, TreemapPart};

const MODULES: [(&str, u64, [u64; 6]); 25] = [
    ("Com", 182_000, [150, 20, 8, 0, 4, 0]),
    ("Rte", 141_000, [90, 40, 6, 5, 0, 0]),
    ("Dio", 96_000, [30, 60, 4, 2, 0, 0]),
    ("Can", 88_000, [60, 12, 10, 6, 0, 0]),
    ("Lin", 64_000, [50, 8, 4, 2, 0, 0]),
    ("Lamp", 52_000, [10, 5, 30, 7, 0, 0]),
    ("Seat", 47_000, [30, 10, 5, 2, 0, 0]),
    ("Horn", 33_000, [20, 8, 3, 2, 0, 0]),
    ("Mem", 29_000, [10, 4, 12, 3, 0, 0]),
    ("Wdg", 21_000, [14, 4, 2, 1, 0, 0]),
    ("Adc", 18_500, [10, 6, 2, 1, 0, 0]),
    ("Pwm", 16_000, [10, 4, 1, 1, 0, 0]),
    ("Spi", 14_000, [9, 3, 1, 1, 0, 0]),
    ("Fls", 12_500, [8, 3, 1, 0, 0, 0]),
    ("Eep", 11_000, [7, 2, 1, 1, 0, 0]),
    ("Gpt", 9_800, [6, 2, 1, 1, 0, 0]),
    ("Icu", 8_200, [5, 2, 1, 0, 0, 0]),
    ("Ocu", 7_100, [4, 2, 1, 0, 0, 0]),
    ("Fee", 6_300, [4, 1, 1, 0, 0, 0]),
    ("Nvm", 5_600, [3, 1, 1, 0, 0, 0]),
    ("Dem", 4_900, [3, 1, 0, 1, 0, 0]),
    ("Det", 4_100, [2, 1, 1, 0, 0, 0]),
    ("Os", 3_300, [2, 1, 0, 0, 0, 0]),
    ("Sched", 2_400, [2, 0, 0, 0, 0, 0]),
    ("(not attributed)", 1_800, [0, 0, 0, 0, 1, 1]),
];

const PART_NAMES: [&str; 5] = ["init.o", "main.o", "cfg.o", "rx.o", "tx.o"];
const SIZES: [(f32, f32); 3] = [(620., 400.), (320., 260.), (320., 110.)];

pub struct TreemapPage {
    groups: Rc<[TreemapGroup]>,
    layouts: Vec<Rc<[rusty_automotive::space::Tile]>>,
    picked: Option<u32>,
}

fn part_kinds(group: usize, part: usize) -> [u64; 6] {
    let mut kinds = [0; 6];
    kinds[(group + part) % 4] = 1_000;
    kinds[(group + 2 * part) % 4] += 400;
    kinds
}

fn groups() -> Vec<TreemapGroup> {
    MODULES
        .iter()
        .enumerate()
        .map(|(index, (name, _, by_kind))| TreemapGroup {
            id: index as u32,
            name: (*name).into(),
            by_kind: by_kind.map(|share| share * 100),
            parts: PART_NAMES
                .iter()
                .enumerate()
                .map(|(part, name)| TreemapPart {
                    id: part as u32,
                    name: (*name).into(),
                    by_kind: part_kinds(index, part),
                })
                .collect(),
        })
        .collect()
}

fn items() -> Vec<Item> {
    MODULES
        .iter()
        .enumerate()
        .map(|(index, (_, value, _))| Item {
            id: index as u32,
            value: *value,
            parts: PART_NAMES
                .iter()
                .enumerate()
                .map(|(part, _)| Part {
                    id: part as u32,
                    value: value / (2 + part as u64 * 2),
                })
                .collect(),
        })
        .collect()
}

impl TreemapPage {
    pub fn new() -> Self {
        let items = items();
        let layouts = SIZES
            .iter()
            .map(|(w, h)| layout(&items, Rect::new(0., 0., f64::from(*w), f64::from(*h))).into())
            .collect();
        Self {
            groups: groups().into(),
            layouts,
            picked: Some(2),
        }
    }
}

impl Render for TreemapPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let page = cx.entity();
        let boxes = SIZES
            .iter()
            .zip(&self.layouts)
            .enumerate()
            .map(|(index, ((w, h), tiles))| {
                let page = page.clone();
                div().w(px(*w)).h(px(*h)).flex_none().child(
                    Treemap::new(("treemap", index), self.groups.clone(), tiles.clone())
                        .selected(self.picked)
                        .on_event(move |event, _, cx| pick(&page, event, cx)),
                )
            });
        let boxes: Vec<_> = boxes.collect();
        let mut boxes = boxes.into_iter();
        let wide = boxes.next();
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .gap(t.spacing.lg)
            .p(t.spacing.xl)
            .bg(t.colors.surface)
            .children(wide)
            .child(div().flex().flex_col().gap(t.spacing.lg).children(boxes))
    }
}

fn pick(page: &Entity<TreemapPage>, event: TreemapEvent, cx: &mut App) {
    let TreemapEvent::Pick(id) = event;
    page.update(cx, |page, cx| {
        page.picked = Some(id);
        cx.notify();
    });
}
