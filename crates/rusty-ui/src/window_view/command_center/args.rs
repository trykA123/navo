use rusty_core::{Command, FsError, FsErrorKind, Location, typed_location};
use rusty_fs::FsEntry;
use rusty_l10n::t;

use super::super::FerriteWindow;
use super::super::hand_off::look_up;
use super::super::path_edit::is_folder;
use super::Step;
use crate::command_center::{Query, Row, Target};
use crate::commands::ArgKind;
use crate::prelude::*;

const ELLIPSIS: char = '…';

pub(super) enum Value {
    Typed(String),
    Picked(Location),
}

type Found = Option<Result<FsEntry, FsError>>;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

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
    let id = step.name.replace('_', "-");
    let text = format!("{id} {{ {field}: {} }}", quoted(value.trim()));
    Command::parse(&text).map_err(|error| error.to_string())
}

pub(super) fn picked(row: Option<&Row>) -> Option<Location> {
    match row {
        Some(Row::TopHit(hit) | Row::Hit(hit)) => match &hit.target {
            Target::GoTo(location)
            | Target::Open {
                location,
                folder: true,
            } => Some(location.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn is_path_input(step: Option<&Step>, query: &str) -> bool {
    match step.map(|step| step.kind) {
        Some(ArgKind::Path) => !query.trim().is_empty(),
        Some(ArgKind::Text) => false,
        None => Query::parse(query).path.is_some(),
    }
}

fn resolve(
    (step, value): (&Step, Value),
    here: &Location,
) -> Result<(Command, Option<Location>), String> {
    let (text, location) = match (step.kind, value) {
        (ArgKind::Text, Value::Typed(text)) => (text, None),
        (_, Value::Picked(location)) => (location.as_str().to_owned(), Some(location)),
        (ArgKind::Path, Value::Typed(text)) => {
            let location = typed_location(&text, here, &env)?;
            (location.as_str().to_owned(), Some(location))
        }
    };
    Ok((argument_command(step, &text)?, location))
}

struct Checked {
    generation: u64,
    name: &'static str,
    command: Command,
    location: Location,
    found: Found,
}

impl FerriteWindow {
    pub(super) fn choose_argument(
        &mut self,
        (name, kind, title): (&'static str, ArgKind, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let label = title.trim_end_matches(ELLIPSIS).trim_end().to_owned();
        let step = Step {
            name,
            kind,
            label: label.into(),
            previous: open.center.read(cx).query(cx),
        };
        self.enter_step(Some(step), window, cx);
    }

    fn enter_step(&mut self, step: Option<Step>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        open.step = step;
        open.error = None;
        let center = open.center.clone();
        center.update(cx, |center, cx| center.set_query("", window, cx));
        self.query_changed("", window, cx);
    }

    pub(super) fn leave_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(previous) = self
            .command_center
            .open
            .as_ref()
            .and_then(|open| open.step.as_ref().map(|step| step.previous.clone()))
        else {
            return;
        };
        self.enter_step(None, window, cx);
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let center = open.center.clone();
        center.update(cx, |center, cx| center.set_query(&previous, window, cx));
        self.query_changed(&previous, window, cx);
        if let Some(open) = self.command_center.open.as_mut() {
            open.after_step = true;
        }
    }

    pub(super) fn run_argument(
        &mut self,
        (step, value): (&Step, Value),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let here = self.browser.state().active_tab().location().clone();
        match resolve((step, value), &here) {
            Err(message) => self.argument_error(message.into(), window, cx),
            Ok((command, Some(location))) => {
                self.check_argument((step.name, command, location), window, cx)
            }
            Ok((command, None)) => self.finish_argument((step.name, &command), window, cx),
        }
    }

    fn argument_error(
        &mut self,
        message: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.command_center.open.as_mut() {
            open.error = Some(message);
        }
        self.push_command_center(window, cx);
    }

    fn check_argument(
        &mut self,
        (name, command, location): (&'static str, Command, Location),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let generation = open.search.generation();
        let provider = self.provider.clone();
        cx.spawn_in(window, async move |this, cx| {
            let found = look_up(provider, location.clone(), cx).await;
            let checked = Checked {
                generation,
                name,
                command,
                location,
                found,
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.argument_checked(checked, window, cx);
            });
        })
        .detach();
    }

    fn argument_checked(&mut self, checked: Checked, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        if open.search.generation() != checked.generation {
            return;
        }
        let path = checked.location.as_str().to_owned();
        match &checked.found {
            Some(Err(error)) if error.kind == FsErrorKind::NotFound => {
                self.argument_error(
                    t!("command-center-arg-missing", path = path).into(),
                    window,
                    cx,
                );
            }
            Some(Ok(entry)) if !is_folder(entry) => {
                self.argument_error(
                    t!("command-center-arg-not-folder", path = path).into(),
                    window,
                    cx,
                );
            }
            _ => self.finish_argument((checked.name, &checked.command), window, cx),
        }
    }

    fn finish_argument(
        &mut self,
        (name, command): (&'static str, &Command),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_command(name, cx);
        self.run_core(command, window, cx);
    }

    pub(super) fn command_center_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        if open.step.is_some() {
            return self.leave_step(window, cx);
        }
        let center = open.center.clone();
        if open.after_step || center.read(cx).query(cx).is_empty() {
            return self.close_command_center(window, cx);
        }
        center.update(cx, |center, cx| center.set_query("", window, cx));
        self.query_changed("", window, cx);
    }
}
