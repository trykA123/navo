use std::iter::Peekable;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::str::Chars;

use rusty_ui::settings::{Field, Value, Values};
use rusty_ui::window_view::TerminalHooks;

pub fn command_line(
    provider: &str,
    template: Option<&str>,
    folder: &Path,
) -> Result<(String, Vec<String>), String> {
    let folder = folder.to_string_lossy().into_owned();
    let (program, args) = match provider {
        "windows-terminal" => ("wt.exe", vec!["-d".to_owned(), folder]),
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
            (
                program,
                vec!["-NoExit".to_owned(), "-WorkingDirectory".to_owned(), folder],
            )
        }
        "cmd" => (
            "cmd.exe",
            vec!["/K".to_owned(), "cd".to_owned(), "/d".to_owned(), folder],
        ),
        "custom" => {
            let mut words = split(template.unwrap_or_default())?.into_iter();
            let program = words
                .next()
                .filter(|word| !word.is_empty())
                .ok_or_else(|| rusty_l10n::tr("terminal-command-empty"))?;
            return Ok((
                program.replace("{path}", &folder),
                words.map(|word| word.replace("{path}", &folder)).collect(),
            ));
        }
        _ => return Err(rusty_l10n::tr("terminal-provider-unknown")),
    };
    Ok((program.to_owned(), args))
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
    let (program, args) = if cfg!(unix) && provider != "custom" {
        let terminal =
            std::env::var("TERMINAL").unwrap_or_else(|_| "x-terminal-emulator".to_owned());
        command_line("custom", Some(&terminal), folder)?
    } else {
        command_line(provider, template, folder)?
    };
    let mut command = Command::new(program);
    command.args(args).current_dir(folder);
    Ok(command)
}

pub fn hooks(values: &Values) -> TerminalHooks {
    let text = |field| match values.get(field) {
        Value::Text(text) => Some(text.to_string()),
        _ => None,
    };
    let provider = text(Field::TerminalProvider).unwrap_or_else(|| "windows-terminal".to_owned());
    let template = text(Field::TerminalCommand);
    TerminalHooks {
        open: Rc::new(move |folder| {
            let mut command =
                launch_command(&provider, template.as_deref(), Path::new(folder.as_str()))?;
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt as _;
                command.creation_flags(0x0000_0008 | 0x0000_0200);
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt as _;
                command.process_group(0);
            }
            let mut child = command.spawn().map_err(|error| {
                rusty_l10n::t!("terminal-spawn-error", error = error.to_string())
            })?;
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }),
    }
}

#[cfg(test)]
mod tests;
