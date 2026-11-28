//! Shell prompt-hook injection and safe folder-sync quoting for supported shells
//! (`pwsh`, Windows PowerShell and `cmd.exe`). No user profile files are modified.

use std::path::Path;

use super::{Nonce, SyncWait};
use crate::shells::{ShellKind, ShellSpec};

/// Extra launch arguments and environment variables injected when spawning an integrated shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellHook {
    extra_args: Vec<String>,
    extra_env: Vec<(String, String)>,
}

impl ShellHook {
    /// Builds the prompt integration hook for `shell`, or `None` when `shell` is not integrated
    /// or already carries an explicit command/file argument.
    pub fn for_shell(shell: &ShellSpec, nonce: &Nonce) -> Option<Self> {
        match shell.kind {
            ShellKind::Pwsh | ShellKind::WindowsPowerShell => {
                if has_powershell_command_flag(&shell.args) {
                    return None;
                }
                Some(Self {
                    extra_args: vec![
                        String::from("-NoExit"),
                        String::from("-Command"),
                        powershell_prompt_script(nonce),
                    ],
                    extra_env: Vec::new(),
                })
            }
            ShellKind::Cmd => Some(Self {
                extra_args: Vec::new(),
                extra_env: vec![(
                    String::from("PROMPT"),
                    format!("$E]133;A;navo={}$E\\$P$G", nonce.as_str()),
                )],
            }),
            ShellKind::GitBash
            | ShellKind::Wsl
            | ShellKind::Bash
            | ShellKind::Zsh
            | ShellKind::Fish
            | ShellKind::Other => None,
        }
    }

    /// Extra arguments appended to the shell command line.
    pub fn args(&self) -> &[String] {
        &self.extra_args
    }

    /// Extra environment variables set on the spawned shell process.
    pub fn env(&self) -> &[(String, String)] {
        &self.extra_env
    }
}

/// Formats a shell command that changes directory to `folder`, or rejects unsafe names.
pub fn quote_sync_command(kind: ShellKind, folder: &Path) -> Result<String, SyncWait> {
    match kind {
        ShellKind::Pwsh | ShellKind::WindowsPowerShell => quote_powershell(folder),
        ShellKind::Cmd => quote_cmd(folder),
        ShellKind::GitBash
        | ShellKind::Wsl
        | ShellKind::Bash
        | ShellKind::Zsh
        | ShellKind::Fish
        | ShellKind::Other => Err(SyncWait::NeedsIntegration),
    }
}

fn quote_powershell(folder: &Path) -> Result<String, SyncWait> {
    let text = valid_utf8_path(folder)?;
    // PowerShell also treats Unicode smart single quotes as string delimiters.
    if text
        .chars()
        .any(|ch| matches!(ch, '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}'))
    {
        return Err(SyncWait::UnusualName);
    }
    let escaped = text.replace('\'', "''");
    Ok(format!("Set-Location -LiteralPath '{escaped}'\r"))
}

fn quote_cmd(folder: &Path) -> Result<String, SyncWait> {
    let text = valid_utf8_path(folder)?;
    if text.chars().any(|ch| matches!(ch, '"' | '%' | '^' | '!')) {
        return Err(SyncWait::UnusualName);
    }
    Ok(format!("cd /d \"{text}\"\r"))
}

fn valid_utf8_path(folder: &Path) -> Result<&str, SyncWait> {
    let text = folder.to_str().ok_or(SyncWait::UnusualName)?;
    if text.is_empty()
        || text
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'))
    {
        return Err(SyncWait::UnusualName);
    }
    Ok(text)
}

fn has_powershell_command_flag(args: &[String]) -> bool {
    const FLAGS: &[&str] = &[
        "-command",
        "-c",
        "-file",
        "-f",
        "-encodedcommand",
        "-e",
        "-ec",
    ];
    args.iter().any(|arg| {
        let flag = arg.split_once(':').map_or(arg.as_str(), |(head, _)| head);
        FLAGS.iter().any(|want| flag.eq_ignore_ascii_case(want))
    })
}

/// PowerShell 5.1 and 7 compatible `-Command` hook: uses only single quotes and `[char]` so
/// Windows command-line quoting never alters it, works under StrictMode, preserves any profile
/// `prompt` and `$LASTEXITCODE`, and emits `133;D;<code>` + `133;A;navo=<nonce>` before prompts.
fn powershell_prompt_script(nonce: &Nonce) -> String {
    let nonce = nonce.as_str();
    format!(
        "$global:__navo_p = Get-Item -Path Function:prompt -ErrorAction SilentlyContinue; \
         $global:__navo_r = @($false); \
         function global:prompt {{ \
           $ok = $?; \
           $lv = Get-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue; \
           $ec = if ($null -ne $lv) {{ $lv.Value }} else {{ $null }}; \
           $e = [string][char]27; \
           $s = [string][char]92; \
           $pre = if ($global:__navo_r[0]) {{ \
             $c = if ($ok) {{ 0 }} elseif ($null -ne $ec -and $ec -ne 0) {{ $ec }} else {{ 1 }}; \
             $e + ']133;D;' + $c + $e + $s \
           }} else {{ \
             $global:__navo_r[0] = $true; '' \
           }}; \
           $body = if ($null -ne $global:__navo_p) {{ \
             & $global:__navo_p.ScriptBlock \
           }} else {{ \
             'PS ' + $executionContext.SessionState.Path.CurrentLocation + ('>' * ($nestedPromptLevel + 1)) + ' ' \
           }}; \
           if ($null -ne $lv) {{ $global:LASTEXITCODE = $ec }}; \
           $pre + $e + ']133;A;navo={nonce}' + $e + $s + $body \
         }}"
    )
}
