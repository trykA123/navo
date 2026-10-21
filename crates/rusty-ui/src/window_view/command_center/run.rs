use rusty_core::Command;

use super::super::FerriteWindow;
use super::args::{Value, picked};
use super::open::{Place, Way};
use crate::command_center::{Group, Hit, Row, Target, completion};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

fn prefix(group: Group) -> Option<char> {
    match group {
        Group::Commands => Some('>'),
        Group::Folders | Group::Files => Some('/'),
        Group::Module(_) => None,
    }
}

impl FerriteWindow {
    pub(super) fn run_on_enter(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        if open.search.shown != open.search.generation() {
            open.run_when_shown = Some(way);
            return;
        }
        self.run_selected(way, window, cx);
    }

    pub(super) fn run_selected(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        if let Some(step) = open.step.clone() {
            if !way.runs_commands() {
                return;
            }
            let typed = open.center.read(cx).query(cx).to_string();
            let value =
                picked(open.selection.row(&open.rows)).map_or(Value::Typed(typed), Value::Picked);
            return self.run_argument((&step, value), window, cx);
        }
        match open.selection.row(&open.rows).cloned() {
            Some(Row::TopHit(hit) | Row::Hit(hit)) => self.run_hit((hit, way), window, cx),
            Some(Row::ShowAll { group, .. }) if way.runs_commands() => {
                self.show_all(group, window, cx);
            }
            Some(Row::ShowAll { .. }) => {}
            Some(Row::Header(_) | Row::Hint) | None => {}
        }
    }

    fn run_hit(&mut self, (hit, way): (Hit, Way), window: &mut Window, cx: &mut Context<Self>) {
        let place = match hit.target {
            Target::Open { location, folder } => Place { location, folder },
            Target::GoTo(location) => Place {
                location,
                folder: true,
            },
            _ if !way.runs_commands() => return,
            target => return self.run_target((target, hit.title), window, cx),
        };
        self.open_place((place, way), window, cx);
    }

    fn run_target(
        &mut self,
        (target, title): (Target, SharedString),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            Target::Action(name) => self.run_action(name, window, cx),
            Target::Core(command) => self.run_core(&command, window, cx),
            Target::Arg(name, kind) => self.choose_argument((name, kind, &title), window, cx),
            Target::ShowAll(group) => self.show_all(group, window, cx),
            Target::Open { .. } | Target::GoTo(_) => {}
        }
    }

    fn run_action(&mut self, name: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self
            .command_center
            .open
            .as_ref()
            .and_then(|open| open.restore.clone());
        self.close_command_center(window, cx);
        let Ok(action) = cx.build_action(name, None) else {
            tracing::error!(name, "command center action is not registered");
            self.show_notice(StatusNotice {
                text: rusty_l10n::tr("command-center-action-missing").into(),
                is_error: true,
            });
            return;
        };
        self.record_command(name, cx);
        let pane = self.browser.state().focused_pane();
        let focus = restore.unwrap_or_else(|| self.handle(pane, cx));
        window.defer(cx, move |window, cx| {
            window.focus(&focus, cx);
            focus.dispatch_action(action.as_ref(), window, cx);
        });
    }

    pub(super) fn record_command(&self, name: &str, cx: &mut Context<Self>) {
        let record = self.command_center.hooks.record_command.clone();
        let name = name.to_owned();
        cx.background_executor()
            .spawn(async move { record(&name) })
            .detach();
    }

    pub(super) fn run_core(
        &mut self,
        command: &Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_command_center(window, cx);
        self.apply(command, window, cx);
    }

    pub(super) fn complete_path(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.command_center.open.as_ref() else {
            return false;
        };
        let query = open.center.read(cx).query(cx);
        let (true, Some(Row::TopHit(hit) | Row::Hit(hit))) = (
            super::args::is_path_input(open.step.as_ref(), &query),
            open.selection.row(&open.rows),
        ) else {
            return false;
        };
        let Some(text) = completion(&query, hit) else {
            return false;
        };
        let center = open.center.clone();
        center.update(cx, |center, cx| center.set_query(&text, window, cx));
        self.query_changed(&text, window, cx);
        true
    }

    fn show_all(&mut self, group: Group, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(prefix), Some(open)) = (prefix(group), self.command_center.open.as_ref()) else {
            return;
        };
        let query = open.center.read(cx).query(cx);
        let rest = query.trim_start_matches(['>', '/']);
        let text = format!("{prefix}{rest}");
        let center = open.center.clone();
        center.update(cx, |center, cx| center.set_query(&text, window, cx));
        self.query_changed(&text, window, cx);
    }
}
