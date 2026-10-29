use std::iter::Peekable;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::str::Chars;

use rusty_ui::settings::{Field, Value, Values};
use rusty_ui::window_view::TerminalHooks;

mod system;

pub fn command_line(
    provider: &str,
    template: Option<&str>,
    folder: &Path,
) -> Result<(String, Vec<String>), String> {
    let folder = folder.to_string_lossy().into_owned();
    let (program, args) = match provider {
        "windows-terminal" => ("wt.exe", vec!["-d".to_owned(), ".".to_owned()]),
        "powershell" => {
            let search = crate::editor::Search {
                path: std::env::var("PATH").unwrap_or_default(),
                extensions: ".EXE".to_owned(),
            };
            let program = if crate::editor::resolve("pwsh.exe", &search).is_some() {
                "pwsh.exe"
            } else {
                "powershell.exe"
            };
            (program, vec!["-NoExit".to_owned()])
        }
        "cmd" => ("cmd.exe", vec![]),
        "custom" => return custom_command(template.unwrap_or_default(), &folder),
        _ => return Err(rusty_l10n::tr("terminal-provider-unknown")),
    };
    Ok((program.to_owned(), args))
}

fn custom_command(template: &str, folder: &str) -> Result<(String, Vec<String>), String> {
    let mut words = split(template)?.into_iter();
    let program = words
        .next()
        .filter(|word| !word.is_empty())
        .ok_or_else(|| rusty_l10n::tr("terminal-command-empty"))?;
    if program.contains("{path}") {
        return Err(rusty_l10n::tr("terminal-command-path"));
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
            return Err(rusty_l10n::tr("terminal-command-path"));
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

fn program_identity(program: &str) -> String {
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

fn is_shell_program(program: &str) -> bool {
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

fn split(template: &str) -> Result<Vec<String>, String> {
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

fn word(chars: &mut Peekable<Chars<'_>>) -> Result<String, String> {
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
        return Err(rusty_l10n::tr("terminal-command-quotes"));
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
) -> Result<Command, String> {
    let (program, args) = if provider == "system" || (cfg!(unix) && provider != "custom") {
        system::terminal_command(folder)?
    } else {
        command_line(provider, template, folder)?
    };
    let mut command = Command::new(program);
    command.args(args).current_dir(folder);
    Ok(command)
}

pub fn hooks(values: &Values) -> TerminalHooks {
    let provider = values.terminal_provider();
    let template = match values.get(Field::TerminalCommand) {
        Value::Text(text) => Some(text.to_string()),
        _ => None,
    };
    TerminalHooks {
        open: Rc::new(move |folder| {
            let mut command =
                launch_command(provider, template.as_deref(), Path::new(folder.as_str()))?;
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
            let mut child = command.spawn().map_err(|error| {
                rusty_l10n::t!("terminal-spawn-error", error = error.to_string())
            })?;
            let _ = std::thread::Builder::new()
                .name("navo-terminal-wait".to_owned())
                .spawn(move || {
                    let _ = child.wait();
                });
            Ok(())
        }),
    }
}

#[cfg(test)]
mod tests;
