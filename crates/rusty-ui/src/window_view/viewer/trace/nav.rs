use rusty_automotive::{Base, IdStat};

use super::text::matches;
use super::{Filter, TraceView};
use crate::prelude::*;

#[derive(Default)]
pub(super) struct Nav {
    pub typed: String,
    pub by_id: bool,
    pub items: Vec<Option<u32>>,
    pub selected: usize,
}

pub(super) fn items(
    stats: &[IdStat],
    typed: &str,
    (by_id, base): (bool, Base),
) -> Vec<Option<u32>> {
    let mut keys: Vec<u32> = (0..stats.len())
        .filter_map(|at| u32::try_from(at).ok())
        .filter(|key| {
            stats
                .get(*key as usize)
                .is_some_and(|stat| matches(stat, typed, base))
        })
        .collect();
    let stat = |key: &u32| stats.get(*key as usize);
    if by_id {
        keys.sort_by_key(|key| stat(key).map(|stat| (stat.id, stat.channel)));
    } else {
        keys.sort_by_key(|key| {
            stat(key).map(|stat| (std::cmp::Reverse(stat.count), stat.id, stat.channel))
        });
    }
    std::iter::once(None)
        .chain(keys.into_iter().map(Some))
        .collect()
}

impl TraceView {
    pub(super) fn refresh_nav(&mut self) {
        let chosen = self.nav.items.get(self.nav.selected).copied();
        self.nav.items = items(
            &self.trace.stats,
            &self.nav.typed,
            (self.nav.by_id, self.trace.notation().base),
        );
        let found =
            chosen.and_then(|chosen| self.nav.items.iter().position(|item| *item == chosen));
        self.nav.selected = found.unwrap_or(0);
    }

    pub(super) fn type_char(&mut self, typed: char, cx: &mut Context<Self>) {
        self.nav.typed.push(typed);
        self.nav.selected = 1;
        self.retyped(cx);
    }

    pub(super) fn erase_char(&mut self, cx: &mut Context<Self>) {
        self.nav.typed.pop();
        self.retyped(cx);
    }

    fn retyped(&mut self, cx: &mut Context<Self>) {
        self.nav.items = items(
            &self.trace.stats,
            &self.nav.typed,
            (self.nav.by_id, self.trace.notation().base),
        );
        let last = self.nav.items.len().saturating_sub(1);
        self.nav.selected = self.nav.selected.min(last);
        self.nav_scroll.scroll_to(self.nav.selected);
        cx.notify();
    }

    pub(super) fn toggle_sort(&mut self, cx: &mut Context<Self>) {
        self.nav.by_id = !self.nav.by_id;
        self.refresh_nav();
        self.nav_scroll.scroll_to(self.nav.selected);
        cx.notify();
    }

    pub(super) fn pick_nav(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(item) = self.nav.items.get(at).copied() else {
            return;
        };
        self.nav.selected = at;
        self.set_filter(item, cx);
    }

    pub(super) fn set_filter(&mut self, key: Option<u32>, cx: &mut Context<Self>) {
        let row = self.actual(self.cursor);
        self.filter = key.map(|key| Filter {
            key,
            rows: self.trace.rows_with(key),
        });
        self.generation += 1;
        self.fetched = None;
        self.fetching = None;
        let shown = row.map_or(0, |row| self.shown_of(row));
        self.move_cursor(shown, cx);
        self.scroll
            .scroll_to_top(usize::try_from(self.cursor).unwrap_or(usize::MAX));
    }

    pub(super) fn filter_name(&self) -> Option<String> {
        let filter = self.filter.as_ref()?;
        let stat = self.trace.stats.get(filter.key as usize)?;
        Some(super::text::stat_name(stat, self.trace.notation().base))
    }
}
