use gpui_kit::Focusable;
use gpui_kit::component::input::InputEvent;

use super::loaded::Loaded;
use super::scope::Scope;
use super::select::Table;
use super::text::{address, parse_address};
use super::{GoTo, MapView};
use crate::prelude::*;
use crate::primitives::text_field_state;

fn find_symbol(loaded: &Loaded, at: u64, preferred: Option<&Option<String>>) -> Option<usize> {
    let index = loaded.model.symbol_index();
    let spaces = preferred.into_iter().chain(
        loaded
            .spaces
            .iter()
            .filter(|space| Some(*space) != preferred),
    );
    let found = spaces
        .into_iter()
        .find_map(|space| index.symbol_at(at, space.as_deref()))?;
    loaded
        .model
        .symbols
        .iter()
        .position(|symbol| std::ptr::eq(symbol, found))
}

impl MapView {
    pub(super) fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loaded().is_none() || self.go_to.is_some() {
            return;
        }
        let input = text_field_state("Hex address, like 80001000", window, cx);
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.submit_go_to(window, cx);
            }
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.go_to = Some(GoTo {
            input,
            error: None,
            _enter: enter,
            _find: None,
        });
        cx.notify();
    }

    pub(super) fn close_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.go_to.take().is_none() {
            return false;
        }
        window.focus(&self.focus, cx);
        cx.notify();
        true
    }

    fn fail_go_to(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(go_to) = &mut self.go_to {
            go_to.error = Some(error.into());
            go_to._find = None;
        }
        cx.notify();
    }

    fn submit_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(go_to), Some(loaded)) = (&self.go_to, self.loaded().cloned()) else {
            return;
        };
        let typed = go_to.input.read(cx).value().to_string();
        let at = match parse_address(&typed) {
            Ok(at) => at,
            Err(error) => return self.fail_go_to(error, cx),
        };
        let preferred = self
            .selected()
            .filter(|_| self.table() == Table::Symbols)
            .and_then(|row| loaded.model.symbols.get(row))
            .map(|symbol| symbol.address_space.clone());
        let task = cx.spawn_in(window, async move |this, cx| {
            let lookup =
                cx.background_spawn(async move { find_symbol(&loaded, at, preferred.as_ref()) });
            let found = lookup.await;
            let _ = this.update_in(cx, |view, window, cx| view.went_to((at, found), window, cx));
        });
        if let Some(go_to) = &mut self.go_to {
            go_to._find = Some(task);
        }
    }

    fn went_to(
        &mut self,
        (at, found): (u64, Option<usize>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(symbol) = found.and_then(|symbol| u32::try_from(symbol).ok()) else {
            return self.fail_go_to(format!("No symbol holds {}", address(at)), cx);
        };
        self.close_go_to(window, cx);
        self.show_symbol(symbol, cx);
        self.go_address = Some(at);
        if !self.inspector.read(cx).is_open() {
            self.toggle_inspector(cx);
        }
    }

    fn show_symbol(&mut self, symbol: u32, cx: &mut Context<Self>) {
        let in_view = self.table() == Table::Symbols;
        if let Some(row) = in_view
            .then(|| self.rows.iter().position(|at| *at == symbol))
            .flatten()
        {
            self.move_cursor(row, cx);
            return;
        }
        self.wanted_row = Some(symbol);
        self.query.table = Table::Symbols;
        self.query.text.clear();
        self.query.scope = Scope::All;
        self.nav_cursor = 0;
        self.rows = std::sync::Arc::new(Vec::new());
        self.run_select(cx);
    }
}
