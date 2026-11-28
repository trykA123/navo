use std::io;
use std::iter::Peekable;
use std::path::Path;
use std::process::{Command, Stdio};
use std::str::Chars;
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

use navo_store::Terminal as TerminalSettings;
use navo_ui::settings::{Field, Value, Values};
use navo_ui::window_view::TerminalHooks;

mod system;

/// Why the terminal launcher failed; the hook turns each case into a localized message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalError {
    /// The provider setting names no known terminal.
    UnknownProvider,
    /// The custom command has no program.
    CommandEmpty,
    /// The custom command puts the folder where a shell could run it.
    CommandPath,
    /// The custom command leaves a quotation mark open.
    CommandQuotes,
    /// The operating system refused to start the terminal.
    Spawn(io::ErrorKind),
}

impl TerminalError {
    /// The localized text for this failure, ending with the next step.
    pub fn message(self) -> String {
        match self {
            Self::UnknownProvider => navo_l10n::tr("terminal-provider-unknown"),
            Self::CommandEmpty => navo_l10n::tr("terminal-command-empty"),
            Self::CommandPath => navo_l10n::tr("terminal-command-path"),
            Self::CommandQuotes => navo_l10n::tr("terminal-command-quotes"),
            Self::Spawn(kind) => navo_l10n::t!("terminal-spawn-error", error = kind.to_string()),
        }
    }
}

pub fn command_line(
    provider: &str,
    template: Option<&str>,
    folder: &Path,
) -> Result<(String, Vec<String>), TerminalError> {
    let folder = folder.to_string_lossy().into_owned();
    let (program, args) = match provider {
        "windows-terminal" => ("wt.exe", vec!["-d".to_owned(), ".".to_owned()]),
        "powershell" => {
            static PROGRAM: OnceLock<&str> = OnceLock::new();
            let program = *PROGRAM.get_or_init(powershell);
            (program, vec!["-NoExit".to_owned()])
        }
        "cmd" => ("cmd.exe", vec![]),
        "custom" => return custom_command(template.unwrap_or_default(), &folder),
        _ => return Err(TerminalError::UnknownProvider),
    };
    Ok((program.to_owned(), args))
}

fn powershell() -> &'static str {
    let search = crate::editor::Search {
        path: std::env::var("PATH").unwrap_or_default(),
        extensions: ".EXE".to_owned(),
    };
    if crate::editor::resolve("pwsh.exe", &search).is_some() {
        "pwsh.exe"
    } else {
        "powershell.exe"
    }
}

fn custom_command(template: &str, folder: &str) -> Result<(String, Vec<String>), TerminalError> {
    let mut words = split(template)?.into_iter();
    let program = words
        .next()
        .filter(|word| !word.is_empty())
        .ok_or(TerminalError::CommandEmpty)?;
    if program.contains("{path}") {
        return Err(TerminalError::CommandPath);
    }
    let mut shell = is_shell_program(&program);
    let folder = if program_identity(&program) == "wt" {
        folder.replace(';', r"\;")
    } else {
        folder.to_owned()
    };
    let mut args = Vec::new();
    for word in words {
        if word.contains("{path}") && (word != "{path}" || shell) {
            return Err(TerminalError::CommandPath);
        }
        shell |= is_shell_program(&word);
        args.push(if word == "{path}" {
            folder.clone()
        } else {
            word
        });
    }
    Ok((program, args))
}

pub(crate) fn program_identity(program: &str) -> String {
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .trim_end_matches(['.', ' '])
        .to_ascii_lowercase();
    name.rsplit_once('.')
        .filter(|(_, extension)| matches!(*extension, "exe" | "com" | "bat" | "cmd"))
        .map_or(name.as_str(), |(stem, _)| stem)
        .to_owned()
}

pub(crate) fn is_shell_program(program: &str) -> bool {
    program
        .trim_end_matches(['.', ' '])
        .rsplit_once('.')
        .is_some_and(|(_, extension)| {
            extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
        })
        || matches!(
            program_identity(program).as_str(),
            "cmd"
                | "powershell"
                | "pwsh"
                | "sh"
                | "bash"
                | "dash"
                | "zsh"
                | "fish"
                | "ksh"
                | "wsl"
                | "wscript"
                | "cscript"
                | "mshta"
                | "conhost"
        )
}

#[cfg(any(windows, test))]
fn is_console_provider(provider: &str) -> bool {
    matches!(
        program_identity(provider).as_str(),
        "cmd" | "powershell" | "pwsh"
    )
}

#[cfg(any(windows, test))]
fn windows_creation_flags(provider: &str) -> u32 {
    let console = if is_console_provider(provider) {
        0x0000_0010
    } else {
        0x0000_0008
    };
    console | 0x0000_0200
}

pub(crate) fn split(template: &str) -> Result<Vec<String>, TerminalError> {
    let mut chars = template.chars().peekable();
    let mut words = Vec::new();
    loop {
        while chars.next_if(|ch| ch.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            return Ok(words);
        }
        words.push(word(&mut chars)?);
    }
}

fn word(chars: &mut Peekable<Chars<'_>>) -> Result<String, TerminalError> {
    let mut word = String::new();
    let mut quoted = false;
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() && !quoted {
            break;
        }
        if ch == '\\' {
            backslashes(chars, &mut word);
            continue;
        }
        chars.next();
        if ch != '"' {
            word.push(ch);
        } else if quoted && chars.next_if_eq(&'"').is_some() {
            word.push('"');
        } else {
            quoted = !quoted;
        }
    }
    if quoted {
        return Err(TerminalError::CommandQuotes);
    }
    Ok(word)
}

fn backslashes(chars: &mut Peekable<Chars<'_>>, word: &mut String) {
    let mut count = 0;
    while chars.next_if_eq(&'\\').is_some() {
        count += 1;
    }
    if chars.peek() != Some(&'"') {
        word.extend(std::iter::repeat_n('\\', count));
        return;
    }
    word.extend(std::iter::repeat_n('\\', count / 2));
    if count % 2 == 1 {
        chars.next();
        word.push('"');
    }
}

fn launch_command(
    provider: &str,
    template: Option<&str>,
    folder: &Path,
) -> Result<Command, TerminalError> {
    let (program, args) = if provider == "system" || (cfg!(unix) && provider != "custom") {
        system::terminal_command(folder)?
    } else {
        command_line(provider, template, folder)?
    };
    let mut command = Command::new(program);
    command.args(args).current_dir(folder);
    Ok(command)
}

#[derive(Clone, Debug)]
struct TerminalChoice {
    provider: String,
    template: Option<String>,
}

impl Default for TerminalChoice {
    fn default() -> Self {
        Self {
            provider: "system".to_owned(),
            template: None,
        }
    }
}

/// Live external terminal configuration shared between Settings reloads and pane launches.
#[derive(Clone, Default)]
pub struct LiveTerminal {
    choice: Arc<RwLock<TerminalChoice>>,
}

impl LiveTerminal {
    /// Updates the live external terminal settings from the persisted store struct.
    pub fn apply(&self, settings: &TerminalSettings) {
        let mut choice = self.choice.write().unwrap_or_else(PoisonError::into_inner);
        choice.provider.clone_from(&settings.provider);
        choice.template.clone_from(&settings.command);
    }

    /// Updates the live external terminal settings from UI [`Values`].
    pub fn apply_values(&self, values: &Values) {
        let template = match values.get(Field::TerminalCommand) {
            Value::Text(text) if !text.is_empty() => Some(text.to_string()),
            _ => None,
        };
        let mut choice = self.choice.write().unwrap_or_else(PoisonError::into_inner);
        choice.provider = values.terminal_provider().to_owned();
        choice.template = template;
    }

    /// Builds [`TerminalHooks`] reading the latest live settings on every launch.
    #[must_use]
    pub fn hooks(&self) -> TerminalHooks {
        let choice = Arc::clone(&self.choice);
        TerminalHooks {
            open: Arc::new(move |folder| {
                let current = choice
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                spawn_terminal(
                    &current.provider,
                    current.template.as_deref(),
                    Path::new(folder.as_str()),
                )
            }),
        }
    }
}

fn spawn_terminal(provider: &str, template: Option<&str>, folder: &Path) -> Result<(), String> {
    let mut command = launch_command(provider, template, folder).map_err(TerminalError::message)?;
    #[cfg(windows)]
    let detached = {
        use std::os::windows::process::CommandExt as _;
        let flags = windows_creation_flags(&command.get_program().to_string_lossy());
        command.creation_flags(flags);
        flags & 0x0000_0008 != 0
    };
    #[cfg(not(windows))]
    let detached = true;
    if detached {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| TerminalError::Spawn(error.kind()).message())?;
    let _ = std::thread::Builder::new()
        .name("navo-terminal-wait".to_owned())
        .spawn(move || {
            let _ = child.wait();
        });
    Ok(())
}

#[cfg(test)]
mod tests;
