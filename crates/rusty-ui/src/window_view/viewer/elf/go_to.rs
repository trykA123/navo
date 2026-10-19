use gpui_kit::Focusable;
use gpui_kit::component::input::InputEvent;
use rusty_automotive::{Query, Scope};

use super::text::{address, is_wide, parse_address};
use super::{ElfView, GoTo};
use crate::prelude::*;
use crate::primitives::text_field_state;

impl ElfView {
    pub(super) fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.elf().is_none() {
            return;
        }
        let input = text_field_state("Hex address, like 8000400", window, cx);
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

    fn submit_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(go_to) = &self.go_to else {
            return;
        };
        let typed = go_to.input.read(cx).value().to_string();
        match self.symbol_for(&typed) {
            Ok((at, symbol)) => {
                self.close_go_to(window, cx);
                self.show_symbol(symbol, cx);
                self.go_address = Some(at);
                if !self.inspector.read(cx).is_open() {
                    self.toggle_inspector(cx);
                }
            }
            Err(error) => {
                if let Some(go_to) = &mut self.go_to {
                    go_to.error = Some(error.into());
                }
                cx.notify();
            }
        }
    }

    fn symbol_for(&self, typed: &str) -> Result<(u64, u32), String> {
        let at = parse_address(typed)?;
        let elf = self.elf().ok_or("The file is still being read.")?;
        let symbol = elf
            .symbol_at(at)
            .ok_or_else(|| format!("No symbol at or below {}", address(at, is_wide(elf))))?;
        Ok((at, symbol as u32))
    }

    fn show_symbol(&mut self, symbol: u32, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.iter().position(|at| *at == symbol) {
            self.move_cursor(row, cx);
            return;
        }
        self.wanted_symbol = Some(symbol);
        self.query = Query {
            scope: Scope::All,
            text: String::new(),
            ..self.query.clone()
        };
        self.run_select(cx);
    }
}
