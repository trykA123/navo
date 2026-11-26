//! The user-configurable button bar read from `buttonbar.toml`.

use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use navo_core::Location;
use navo_fs::Watch;
use navo_icons::io::{ReadError, read_capped};
use navo_icons::registry::registry;
use navo_icons::spec::{Shape, Tone};
use navo_store::StoreDirs;
use navo_ui::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, MSG_STALE,
    ProgramId, ProgramRun, UserInvocation,
};

use crate::user_commands::model::{Part, files::takes_files};
use crate::user_commands::{Runner, UserCommand};
use crate::watch_file::watch_file;

pub(crate) mod model;

#[cfg(test)]
mod tests;

const FILE: &str = "buttonbar.toml";
const MAX_FILE_BYTES: u64 = 256 * 1024;
const NO_SETTINGS_FOLDER: &str = "no settings folder";
const NOT_READY: &str = "not ready";

type ProgramTable = (u32, Vec<(String, UserCommand)>);

/// Loads `buttonbar.toml`, watches it, and returns the window's button bar hooks.
#[must_use]
pub(crate) fn hooks(dirs: Option<&StoreDirs>, runner: Runner) -> (ButtonBarHooks, Option<Watch>) {
    let Some(dirs) = dirs.filter(|dirs| dirs.ensure().is_ok()) else {
        return (no_hooks(runner), None);
    };
    let file = dirs.button_bar_file();
    let icons_dir = dirs.button_bar_icons_dir();
    let programs = Arc::new(Mutex::new((0u32, Vec::new())));
    let initial = load(&file, &icons_dir, &programs);
    report(&initial);
    let run_program = run_hook(Arc::clone(&programs), runner);
    let edit = Arc::new(|_| Err(NOT_READY.to_owned()));
    let undo = Arc::new(|| Err(NOT_READY.to_owned()));
    let (hooks, sender) = ButtonBarHooks::channel(run_program, edit, undo, location_of(&file));
    let _ = sender.send(initial.clone());
    let watching = watch(&file, &icons_dir, initial, programs, sender);
    if watching.is_none() {
        tracing::warn!(
            "the settings folder is not watched; buttonbar.toml changes apply at the next start"
        );
    }
    (hooks, watching)
}

fn no_hooks(runner: Runner) -> ButtonBarHooks {
    ButtonBarHooks {
        run_program: Arc::new(move |_, _| {
            runner.refuse("", NO_SETTINGS_FOLDER.to_owned());
        }),
        edit: Arc::new(|_| Err(NO_SETTINGS_FOLDER.to_owned())),
        undo: Arc::new(|| Err(NO_SETTINGS_FOLDER.to_owned())),
        file: None,
        news: None,
    }
}

fn load(file: &Path, icons_dir: &Path, programs: &Mutex<ProgramTable>) -> ButtonBarNews {
    let bytes = match read_capped(file, MAX_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(ReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            let mut table = lock(programs);
            table.0 = table.0.wrapping_add(1);
            table.1.clear();
            return ButtonBarNews::Applied {
                items: Vec::new(),
                warnings: Vec::new(),
            };
        }
        Err(ReadError::TooLarge) => {
            return ButtonBarNews::Invalid(format!("{FILE}: file exceeds 256 KiB"));
        }
        Err(ReadError::Io(error)) => {
            return ButtonBarNews::Invalid(format!("{FILE}: {error}"));
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => return ButtonBarNews::Invalid(format!("{FILE}: {error}")),
    };
    match model::parse(&text, None) {
        Ok((parsed, warnings)) => apply_parsed(parsed, warnings, icons_dir, programs),
        Err(errors) => {
            let message = errors
                .first()
                .map_or_else(|| format!("{FILE}: invalid"), ToString::to_string);
            ButtonBarNews::Invalid(message)
        }
    }
}

/// Resolves icons off the UI thread, trims the pack cache, and commits the next program generation.
fn apply_parsed(
    parsed: Vec<model::ParsedItem>,
    warnings: Vec<String>,
    icons_dir: &Path,
    programs: &Mutex<ProgramTable>,
) -> ButtonBarNews {
    let generation = lock(programs).0.wrapping_add(1);
    let mut ctx = ResolveCtx {
        generation,
        icons_dir,
        programs: Vec::new(),
        warnings,
    };
    let items = ctx.resolve_list(parsed);
    registry().trim();
    let mut table = lock(programs);
    table.0 = generation;
    table.1 = ctx.programs;
    ButtonBarNews::Applied {
        items,
        warnings: ctx.warnings,
    }
}

struct ResolveCtx<'a> {
    generation: u32,
    icons_dir: &'a Path,
    programs: Vec<(String, UserCommand)>,
    warnings: Vec<String>,
}

impl ResolveCtx<'_> {
    fn resolve_list(&mut self, items: Vec<model::ParsedItem>) -> Vec<ButtonItem> {
        items
            .into_iter()
            .map(|item| self.resolve_item(item))
            .collect()
    }

    fn resolve_item(&mut self, item: model::ParsedItem) -> ButtonItem {
        let icon = self.resolve_icon(&item);
        let action = match item.kind {
            model::ParsedKind::Navo(id) => ButtonAction::Navo(id),
            model::ParsedKind::User(name) => ButtonAction::User(name),
            model::ParsedKind::Program(command) => self.register_program(&item.name, command),
            model::ParsedKind::Separator => ButtonAction::Separator,
            model::ParsedKind::Menu(children) => ButtonAction::Menu(self.resolve_list(children)),
        };
        ButtonItem {
            name: item.name,
            icon,
            action,
            line: item.line,
        }
    }

    fn resolve_icon(&mut self, item: &model::ParsedItem) -> BarIcon {
        if matches!(item.kind, model::ParsedKind::Separator) {
            return BarIcon::Missing { dashed: false };
        }
        let Some(spec) = &item.icon else {
            return fallback_letters(&item.name);
        };
        let (resolved, warning) = registry().resolve(spec, self.icons_dir);
        if let Some(warning) = warning {
            self.warnings.push(warning);
        }
        match resolved {
            BarIcon::Missing { dashed: false } => fallback_letters(&item.name),
            other => other,
        }
    }

    fn register_program(&mut self, name: &str, command: UserCommand) -> ButtonAction {
        let index = u32::try_from(self.programs.len()).unwrap_or(u32::MAX);
        let line = command_line(&command);
        let takes = takes_files(&command.args);
        self.programs.push((name.to_owned(), command));
        ButtonAction::Program {
            id: ProgramId {
                generation: self.generation,
                index,
            },
            line,
            takes_files: takes,
        }
    }
}

fn fallback_letters(name: &str) -> BarIcon {
    let ch = name
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .or_else(|| name.chars().next());
    let letters = ch
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_default();
    BarIcon::Letters {
        letters,
        shape: Shape::Soft,
        tone: Tone::Muted,
    }
}

fn command_line(command: &UserCommand) -> String {
    let mut out = command.program.clone();
    for token in &command.args {
        out.push(' ');
        for part in &token.parts {
            match part {
                Part::Literal(text) => out.push_str(text),
                Part::Placeholder(placeholder) => {
                    out.push('{');
                    out.push_str(placeholder.name());
                    out.push('}');
                }
            }
        }
    }
    out
}

fn watch(
    path: &Path,
    icons_dir: &Path,
    start: ButtonBarNews,
    programs: Arc<Mutex<ProgramTable>>,
    sender: ButtonBarSender,
) -> Option<Watch> {
    let file = path.to_owned();
    let icons_dir = icons_dir.to_owned();
    let mut last = start;
    watch_file(path, move || {
        let news = load(&file, &icons_dir, &programs);
        if news == last {
            return;
        }
        last = news.clone();
        report(&news);
        let _ = sender.send(news);
    })
}

fn run_hook(programs: Arc<Mutex<ProgramTable>>, runner: Runner) -> ProgramRun {
    Arc::new(move |id: ProgramId, invocation: UserInvocation| {
        let lookup = {
            let table = lock(&programs);
            let idx = usize::try_from(id.index).ok();
            if id.generation == table.0 {
                idx.and_then(|i| table.1.get(i))
                    .map(|(_, cmd)| Ok(cmd.clone()))
                    .unwrap_or_else(|| Err(String::new()))
            } else {
                let stale_name = idx
                    .and_then(|i| table.1.get(i))
                    .map(|(name, _)| name.clone())
                    .unwrap_or_default();
                Err(stale_name)
            }
        };
        match lookup {
            Ok(command) => runner.run(command, invocation),
            Err(name) => runner.refuse(&name, navo_l10n::tr(MSG_STALE)),
        }
    })
}

fn report(news: &ButtonBarNews) {
    match news {
        ButtonBarNews::Applied { warnings, .. } => {
            for warning in warnings {
                tracing::warn!(problem = %warning, "button bar applied with a warning");
            }
        }
        ButtonBarNews::Invalid(message) => {
            tracing::error!(problem = %message, "button bar not applied");
        }
    }
}

fn location_of(path: &Path) -> Option<Location> {
    Location::new(path.to_str()?).ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
