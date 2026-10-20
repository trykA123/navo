use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful, WeakEntity};
use rusty_automotive::{Elf, Section};

use super::text::{Spot, address, class, is_wide, place, source, span};
use super::{ElfView, Focus, NavItem};
use crate::format;
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::side_sheet::{SheetFrame, Side, SideSheet};
use crate::tokens::{self, Theme};

const NAVIGATOR_WIDTH: f32 = 360.;
const INSPECTOR_WIDTH: f32 = 380.;
const ITEM_HEIGHT: f32 = 44.;

fn frame(t: &Theme, window: &Window, (width, side): (f32, Side)) -> SheetFrame {
    let body = window.viewport_size().width - t.spacing.md * 2.;
    SheetFrame::new(body, px(width), side.other()).covering_passive_card()
}

fn caption(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(text.into())
}

fn pick(view: WeakEntity<ElfView>, at: usize) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Navigator;
            view.pick_item(at, cx);
        });
    }
}

fn where_loaded(section: &Section, wide: bool) -> String {
    if section.is_loaded() {
        span(section.address, section.size, wide)
    } else {
        "not loaded".to_owned()
    }
}

fn item_text(elf: &Elf, item: NavItem) -> (String, String, String) {
    let wide = is_wide(elf);
    match item {
        NavItem::All => (
            "All symbols".to_owned(),
            format::bytes(elf.symbols.len() as u64),
            format!(
                "{} sections · {} LOAD segments",
                elf.sections.len().saturating_sub(1),
                elf.load_segments().count()
            ),
        ),
        NavItem::Section(index) => elf
            .sections
            .get(index as usize)
            .map(|section| {
                let flags = section.flag_letters();
                let flags = if flags.is_empty() {
                    section.kind_name()
                } else {
                    flags
                };
                let name = if section.name.is_empty() {
                    format!("#{index}")
                } else {
                    section.name.clone()
                };
                (
                    name,
                    format::size(section.size),
                    format!("{} · {flags}", where_loaded(section, wide)),
                )
            })
            .unwrap_or_default(),
        NavItem::Segment(at) => elf
            .segments
            .get(at)
            .map(|segment| {
                (
                    format!("LOAD {}", segment.flag_letters()),
                    format::size(segment.memory_size),
                    format!(
                        "{} · file {}",
                        span(segment.virtual_address, segment.memory_size, wide),
                        format::size(segment.file_size)
                    ),
                )
            })
            .unwrap_or_default(),
    }
}

fn item_row(
    t: &Theme,
    elf: &Elf,
    (at, item): (usize, NavItem),
    is_selected: bool,
) -> ObservedElement<Stateful<Div>> {
    let (name, size, detail) = item_text(elf, item);
    div()
        .id(("elf-item", at))
        .role(Role::ListItem)
        .aria_label(format!("{name}, {size}, {detail}"))
        .aria_selected(is_selected)
        .test_support()
        .flex()
        .flex_col()
        .justify_center()
        .w_full()
        .h(px(ITEM_HEIGHT))
        .pl(t.spacing.xl)
        .pr(t.spacing.xl * 2.)
        .cursor_pointer()
        .when(is_selected, |row| row.bg(t.colors.selection))
        .child(
            div()
                .flex()
                .gap(t.spacing.md)
                .child(div().flex_1().min_w_0().truncate().child(name))
                .child(
                    div()
                        .flex_none()
                        .font_family(t.typography.numbers_font.clone())
                        .text_color(t.colors.muted)
                        .child(size),
                ),
        )
        .child(
            div()
                .truncate()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(detail),
        )
}

fn pair(t: &Theme, (label, value): (&'static str, String)) -> Div {
    div()
        .flex()
        .justify_between()
        .gap(t.spacing.md)
        .child(div().text_color(t.colors.muted).child(label))
        .child(div().min_w_0().truncate().child(value))
}

fn summary(t: &Theme, elf: &Elf) -> Div {
    let wide = is_wide(elf);
    let rows = [
        ("Machine", elf.machine_name()),
        ("Class", class(elf)),
        ("Type", elf.kind_name().to_owned()),
        ("Entry", address(elf.header.entry, wide)),
        ("Flags", format!("0x{:08X}", elf.header.flags)),
    ];
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.xxs)
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .border_b_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .children(rows.into_iter().map(|row| pair(t, row)))
}

fn section(t: &Theme, title: &'static str, rows: Vec<Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .child(caption(t, title))
        .children(rows)
}

impl ElfView {
    fn navigator_list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        VirtualList::new(
            "elf-items",
            self.nav_items.len(),
            &self.nav_scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let this = entity.read(cx);
                let Some(elf) = this.elf() else {
                    return Vec::new();
                };
                range
                    .filter_map(|at| {
                        let item = *this.nav_items.get(at)?;
                        let is_selected = at == this.nav_cursor;
                        Some(
                            item_row(&t, elf, (at, item), is_selected)
                                .on_click(pick(view.clone(), at))
                                .into_any_element(),
                        )
                    })
                    .collect()
            },
        )
        .row_height(px(ITEM_HEIGHT))
        .label("Sections and segments")
    }

    fn navigator(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<SideSheet> {
        let elf = self.elf()?;
        let this = cx.weak_entity();
        let count = format::count(
            elf.sections.len().saturating_sub(1) as u64,
            "section",
            "sections",
        );
        let title = div()
            .flex()
            .items_baseline()
            .gap(t.spacing.md)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Sections"))
            .child(
                div()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(count),
            );
        Some(
            SideSheet::new(
                "elf-navigator",
                &self.navigator,
                frame(t, window, (NAVIGATOR_WIDTH, Side::Left)),
            )
            .label("Sections")
            .header(title)
            .close("ctrl-b", move |_, cx| {
                let _ = this.update(cx, |view, cx| view.close_navigator(cx));
            })
            .body(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(summary(t, elf))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .py(t.spacing.sm)
                            .child(self.navigator_list(cx)),
                    ),
            )
            .hint(&["↑", "↓"], "Move")
            .hint(&["enter"], "Show symbols")
            .hint(&["escape"], "Close"),
        )
    }

    fn inspector_body(&self, t: &Theme, elf: &Elf) -> Stateful<Div> {
        let body = div()
            .id("elf-inspector-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(t.spacing.xl)
            .p(t.spacing.xl);
        if let Some(parts) = self.pick_body(t, elf) {
            return body.gap(t.spacing.sm).children(parts);
        }
        let Some((at, symbol)) = self.selected() else {
            return body.child(div().text_color(t.colors.muted).child("No symbol selected"));
        };
        let wide = is_wide(elf);
        let mut facts = vec![
            pair(t, ("Address", address(symbol.value, wide))),
            pair(t, ("Size", format!("{} B", format::bytes(symbol.size)))),
            pair(t, ("Section", place(elf, symbol))),
            pair(t, ("Type", symbol.kind.name())),
            pair(t, ("Bind", symbol.bind.name())),
        ];
        if let Some(raw) = elf.raw_name(symbol) {
            facts.push(pair(t, ("Mangled", raw.to_owned())));
        }
        if let Some(at) = self.go_address {
            facts.push(pair(t, ("Went to", address(at, wide))));
        }
        let spot = self.go_address.map_or(Spot::Symbol(at), Spot::Address);
        let line = source(elf, self.lines.as_deref(), spot);
        let source_line = div()
            .id("elf-inspector-source")
            .aria_label(line.clone())
            .test_support()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
            .child(line);
        body.child(section(t, "SYMBOL", facts)).child(section(
            t,
            "SOURCE",
            vec![div().child(source_line)],
        ))
    }

    fn inspector(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<SideSheet> {
        let elf = self.elf()?;
        let this = cx.weak_entity();
        let name = self.pick_name().unwrap_or_else(|| {
            self.selected()
                .map_or_else(String::new, |(_, symbol)| elf.name(symbol).to_owned())
        });
        let title = div()
            .min_w_0()
            .truncate()
            .font_weight(FontWeight::SEMIBOLD)
            .child(name);
        Some(
            SideSheet::new(
                "elf-inspector",
                &self.inspector,
                frame(t, window, (INSPECTOR_WIDTH, Side::Right)),
            )
            .label("Inspector")
            .header(title)
            .close("ctrl-i", move |_, cx| {
                let _ = this.update(cx, |view, cx| view.toggle_inspector(cx));
            })
            .body(self.inspector_body(t, elf)),
        )
    }

    pub(super) fn drawers(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let navigator = if self.navigator.read(cx).is_shown(cx) {
            self.navigator(t, window, cx)
        } else {
            None
        };
        let inspector = if self.inspector.read(cx).is_shown(cx) {
            self.inspector(t, window, cx)
        } else {
            None
        };
        div()
            .absolute()
            .top(t.spacing.xs)
            .bottom(t.spacing.xs)
            .left(t.spacing.md)
            .right(t.spacing.md)
            .children(navigator)
            .children(inspector)
    }
}
