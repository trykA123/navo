use rusty_core::{Command, Location};

use super::super::FerriteWindow;
use super::Step;
use crate::command_center::{Group, Hit, Query, Row, Target, completion};
use crate::commands::ArgKind;
use crate::prelude::*;

const ELLIPSIS: char = '…';

fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn argument_command(step: &Step, value: &str) -> Result<Command, String> {
    let field = match step.kind {
        ArgKind::Path => "location",
        ArgKind::Text => "pattern",
    };
    let text = format!("{} {{ {field}: {} }}", step.name, quoted(value.trim()));
    Command::parse(&text).map_err(|error| error.to_string())
}

struct Place {
    location: Location,
    folder: bool,
    new_tab: bool,
}

fn prefix(group: Group) -> Option<char> {
    match group {
        Group::Commands => Some('>'),
        Group::Folders | Group::Files => Some('/'),
        Group::Module(_) => None,
    }
}

fn split_file(location: &Location) -> Option<(Location, String)> {
    let parent = location.parent()?;
    let text = location.path().to_text();
    let name = text.rsplit(['/', '\\']).find(|part| !part.is_empty())?;
    Some((parent, name.to_owned()))
}

impl FerriteWindow {
    pub(super) fn run_selected(
        &mut self,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        if let Some(step) = open.step.clone() {
            let value = open.center.read(cx).query(cx);
            return self.run_argument((&step, &value), window, cx);
        }
        match open.selection.row(&open.rows).cloned() {
            Some(Row::TopHit(hit) | Row::Hit(hit)) => self.run_hit((hit, new_tab), window, cx),
            Some(Row::ShowAll { group, .. }) => self.show_all(group, window, cx),
            Some(Row::Header(_) | Row::Hint) | None => {}
        }
    }

    fn run_hit(
        &mut self,
        (hit, new_tab): (Hit, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match hit.target {
            Target::Action(name) => {
                self.record_command(name, cx);
                self.run_action(name, window, cx);
            }
            Target::Core(command) => self.run_core(&command, window, cx),
            Target::Arg(name, kind) => {
                let label = hit.title.trim_end_matches(ELLIPSIS).trim_end().to_owned();
                self.enter_step(
                    Some(Step {
                        name,
                        kind,
                        label: label.into(),
                    }),
                    window,
                    cx,
                );
            }
            Target::Open { location, folder } => {
                let place = Place {
                    location,
                    folder,
                    new_tab,
                };
                self.open_location(&place, window, cx);
            }
            Target::GoTo(location) => {
                let place = Place {
                    location,
                    folder: true,
                    new_tab,
                };
                self.open_location(&place, window, cx);
            }
            Target::ShowAll(group) => self.show_all(group, window, cx),
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
            debug_assert!(false, "command center action {name} is not registered");
            return;
        };
        let pane = self.browser.state().focused_pane();
        let focus = restore.unwrap_or_else(|| self.handle(pane, cx));
        window.defer(cx, move |window, cx| {
            window.focus(&focus, cx);
            focus.dispatch_action(action.as_ref(), window, cx);
        });
    }

    fn record_command(&self, name: &str, cx: &mut Context<Self>) {
        let record = self.command_center.hooks.record_command.clone();
        let name = name.to_owned();
        cx.background_executor()
            .spawn(async move { record(&name) })
            .detach();
    }

    fn run_core(&mut self, command: &Command, window: &mut Window, cx: &mut Context<Self>) {
        self.close_command_center(window, cx);
        self.apply(command, window, cx);
    }

    fn run_argument(
        &mut self,
        (step, value): (&Step, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match argument_command(step, value) {
            Ok(command) => {
                self.record_command(step.name, cx);
                self.run_core(&command, window, cx);
            }
            Err(error) => {
                if let Some(open) = self.command_center.open.as_mut() {
                    open.error = Some(error.into());
                }
                self.push_command_center(window, cx);
            }
        }
    }

    fn open_location(&mut self, place: &Place, window: &mut Window, cx: &mut Context<Self>) {
        let Place {
            location,
            folder,
            new_tab,
        } = place;
        let (folder, new_tab) = (*folder, *new_tab);
        self.close_command_center(window, cx);
        let opened = match (folder, new_tab) {
            (true, false) => self
                .browser
                .command(&Command::PaneNavigate {
                    location: location.clone(),
                })
                .is_ok(),
            (true, true) => self.browser.open_tab_at(location.clone(), None),
            (false, _) => split_file(location).is_some_and(|(parent, name)| {
                if new_tab {
                    self.browser.open_tab_at(parent, Some(name))
                } else {
                    self.browser.open_at(parent, name)
                }
            }),
        };
        if opened {
            if !folder {
                self.note_file(location, cx);
            }
            self.focus_core_pane(window, cx);
        }
        cx.notify();
    }

    pub(super) fn complete_path(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.command_center.open.as_ref() else {
            return false;
        };
        let query = open.center.read(cx).query(cx);
        let (true, Some(Row::TopHit(hit) | Row::Hit(hit))) = (
            Query::parse(&query).path.is_some() && open.step.is_none(),
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

    pub(super) fn enter_step(
        &mut self,
        step: Option<Step>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        open.step = step;
        open.error = None;
        let center = open.center.clone();
        center.update(cx, |center, cx| center.set_query("", window, cx));
        self.query_changed("", window, cx);
    }

    pub(super) fn command_center_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let center = open.center.clone();
        if !center.read(cx).query(cx).is_empty() {
            center.update(cx, |center, cx| center.set_query("", window, cx));
            return self.query_changed("", window, cx);
        }
        if open.step.is_some() {
            return self.enter_step(None, window, cx);
        }
        self.close_command_center(window, cx);
    }
}
