//! Prompt hooks and folder-sync quoting for bash, zsh, fish, Git Bash and WSL.
//! Each hook writes one owner-only temp init file (see `super::init`) that restores the
//! user's own startup files, keeps the nonce in a non-exported shell variable and emits
//! the trusted `133;A;navo=<nonce>` marker before every prompt.

use std::path::Path;
use std::sync::Arc;

use super::init::TempInit;
use super::shell::{ShellHook, valid_utf8_path};
use super::{Nonce, SyncWait};
use crate::shells::ShellSpec;

/// bash (also the WSL guest shell): `--init-file` replaces `~/.bashrc`, so source it first.
/// bash ignores `--init-file` in a login shell, so `--login`/`-l` is replaced and emulated.
pub(super) fn bash_hook(shell: &ShellSpec, nonce: &Nonce) -> Option<ShellHook> {
    if has_flag(&shell.args, &["-c", "--norc", "--rcfile", "--init-file"]) {
        return None;
    }
    if has_flag(&shell.args, &["--login", "-l"]) {
        return login_bash_hook(nonce);
    }
    let init = temp_init(&[("init.bash", &bash_script(nonce))])?;
    let file = clean_shell_path(init.file())?.to_owned();
    Some(ShellHook {
        extra_args: vec![String::from("--init-file"), file.clone()],
        extra_env: vec![(String::from("__NAVO_INIT"), file)],
        replace_args: false,
        init: Some(init),
    })
}

/// Git Bash: bash ignores `--init-file` in a `--login` shell, so replace the detected
/// `--login -i` arguments and let the init file perform the login startup itself.
pub(super) fn git_bash_hook(shell: &ShellSpec, nonce: &Nonce) -> Option<ShellHook> {
    if has_flag(&shell.args, &["-c", "--norc", "--rcfile", "--init-file"]) {
        return None;
    }
    login_bash_hook(nonce)
}

/// The login-emulating bash hook shared by `bash --login`/`-l` and Git Bash: the login flags
/// are dropped (bash ignores `--init-file` for them) and the init file sources the profile.
fn login_bash_hook(nonce: &Nonce) -> Option<ShellHook> {
    let init = temp_init(&[("init.bash", &login_bash_script(nonce))])?;
    let file = init_file_env(&init)?;
    Some(ShellHook {
        extra_args: vec![
            String::from("--init-file"),
            file.clone(),
            String::from("-i"),
        ],
        extra_env: vec![(String::from("__NAVO_INIT"), file)],
        replace_args: true,
        init: Some(init),
    })
}

/// zsh: a temporary `ZDOTDIR` whose `.zshrc` restores the original `ZDOTDIR` (or `$HOME`),
/// sources the user's `.zshrc` and appends a `precmd` hook. `.zshenv`, `.zprofile` and
/// `.zlogin` are forwarded too, so the temporary `ZDOTDIR` hides no user startup file.
pub(super) fn zsh_hook(shell: &ShellSpec, nonce: &Nonce) -> Option<ShellHook> {
    if has_flag(&shell.args, &["-c", "-f", "--no-rcs"]) {
        return None;
    }
    let init = temp_init(&[
        (".zshrc", &zsh_script(nonce)),
        (".zshenv", &zsh_forward_script(".zshenv")),
        (".zprofile", &zsh_forward_script(".zprofile")),
        (".zlogin", &zsh_forward_script(".zlogin")),
    ])?;
    let dir = clean_shell_path(init.dir())?.to_owned();
    let original = std::env::var("ZDOTDIR").unwrap_or_default();
    Some(ShellHook {
        extra_args: Vec::new(),
        extra_env: vec![
            (String::from("ZDOTDIR"), dir),
            (String::from("__NAVO_ZDOTDIR"), original),
            (String::from("__NAVO_INIT"), init_file_env(&init)?),
        ],
        replace_args: false,
        init: Some(init),
    })
}

/// fish: `--init-command` runs after the user's `config.fish`, so the normal config is
/// already restored; the temp file only wraps `fish_prompt`.
pub(super) fn fish_hook(shell: &ShellSpec, nonce: &Nonce) -> Option<ShellHook> {
    if has_flag(&shell.args, &["-c", "--command", "-n", "--no-execute"]) {
        return None;
    }
    let init = temp_init(&[("init.fish", &fish_script(nonce))])?;
    let file = clean_shell_path(init.file())?;
    Some(ShellHook {
        extra_args: vec![
            String::from("--init-command"),
            format!("source '{}'", posix_escape(file)),
        ],
        extra_env: vec![(String::from("__NAVO_INIT"), file.to_owned())],
        replace_args: false,
        init: Some(init),
    })
}

/// WSL: keep `wsl.exe -d <distro>` and run bash inside, resolving the Windows temp path
/// with `wslpath -u` at start-up.
pub(super) fn wsl_hook(shell: &ShellSpec, nonce: &Nonce) -> Option<ShellHook> {
    if has_flag(&shell.args, &["-e", "--exec", "--"]) {
        return None;
    }
    let init = temp_init(&[("init.bash", &bash_script(nonce))])?;
    let file = clean_shell_path(init.file())?;
    let launch = format!(
        "exec bash --init-file \"$(wslpath -u '{}')\"",
        posix_escape(file)
    );
    Some(ShellHook {
        extra_args: vec![
            String::from("--"),
            String::from("bash"),
            String::from("-c"),
            launch,
        ],
        extra_env: vec![(String::from("__NAVO_INIT"), file.to_owned())],
        replace_args: false,
        init: Some(init),
    })
}

/// POSIX `cd` with single quotes; an embedded `'` becomes `'\''`.
pub(super) fn quote_posix(folder: &Path) -> Result<String, SyncWait> {
    let text = valid_utf8_path(folder)?;
    Ok(format!("cd -- '{}'\r", posix_escape(text)))
}

/// Git Bash: convert a local Windows path to `/c/...`, then quote POSIX-style.
pub(super) fn quote_git_bash(folder: &Path) -> Result<String, SyncWait> {
    let text = valid_utf8_path(folder)?;
    let converted = to_git_bash_path(text).ok_or(SyncWait::UnusualName)?;
    Ok(format!("cd -- '{}'\r", posix_escape(&converted)))
}

/// WSL: convert the Windows path with `wslpath -u` inside the guest.
pub(super) fn quote_wsl(folder: &Path) -> Result<String, SyncWait> {
    let text = valid_utf8_path(folder)?;
    Ok(format!(
        "cd -- \"$(wslpath -u '{}')\"\r",
        posix_escape(text)
    ))
}

/// Escapes `'` for a POSIX single-quoted string: `it's` -> `it'\''s`.
pub(super) fn posix_escape(text: &str) -> String {
    text.replace('\'', "'\\''")
}

/// `C:\Users\a b` or `c:/Users/a b` -> `/c/Users/a b`, `C:\` -> `/c`; UNC, extended-length
/// (`\\?\`), drive-relative (`C:foo`, `C:`) and non-local paths are unsupported.
fn to_git_bash_path(text: &str) -> Option<String> {
    let drive = text.chars().next()?.to_ascii_lowercase();
    if !drive.is_ascii_alphabetic() || text.as_bytes().get(1) != Some(&b':') {
        return None;
    }
    // Only `X:\` and `X:/` are absolute; `C:foo` and `C:` resolve against the shell's cwd.
    if !matches!(text.as_bytes().get(2), Some(b'\\' | b'/')) {
        return None;
    }
    let tail = text.get(2..)?.replace('\\', "/");
    let tail = tail.trim_matches('/');
    Some(if tail.is_empty() {
        format!("/{drive}")
    } else {
        format!("/{drive}/{tail}")
    })
}

/// The shared bash tail: a never-exported nonce plus a `PROMPT_COMMAND` entry emitting the
/// trusted prompt marker. Appending keeps any existing `PROMPT_COMMAND` working.
fn bash_prompt_block(nonce: &Nonce) -> String {
    format!(
        "unset __navo_nonce\n\
         __navo_nonce='{nonce}'\n\
         __navo_prompt() {{ printf '\\033]133;A;navo=%s\\033\\\\' \"$__navo_nonce\"; }}\n\
         if [ -n \"${{PROMPT_COMMAND:-}}\" ]; then\n\
           PROMPT_COMMAND=\"$PROMPT_COMMAND\"$'\\n'\"__navo_prompt\"\n\
         else\n\
           PROMPT_COMMAND=\"__navo_prompt\"\n\
         fi\n",
        nonce = nonce.as_str()
    )
}

fn bash_script(nonce: &Nonce) -> String {
    format!(
        "# Navo terminal integration (temporary file; removed after the first prompt).\n\
         if [ -f \"$HOME/.bashrc\" ]; then . \"$HOME/.bashrc\"; fi\n\
         unset __NAVO_INIT __NAVO_ZDOTDIR\n\
         {block}",
        block = bash_prompt_block(nonce)
    )
}

fn login_bash_script(nonce: &Nonce) -> String {
    format!(
        "# Navo terminal integration for a login bash (temporary; removed after the first prompt).\n\
         # Replaces --login: performs the login startup itself, then installs the prompt hook.\n\
         if [ -f /etc/profile ]; then . /etc/profile; fi\n\
         __navo_profile=''\n\
         for __navo_f in \"$HOME/.bash_profile\" \"$HOME/.bash_login\" \"$HOME/.profile\"; do\n\
           if [ -f \"$__navo_f\" ]; then . \"$__navo_f\"; __navo_profile=1; break; fi\n\
         done\n\
         if [ -z \"$__navo_profile\" ] && [ -f \"$HOME/.bashrc\" ]; then . \"$HOME/.bashrc\"; fi\n\
         unset __navo_f __navo_profile __NAVO_INIT __NAVO_ZDOTDIR\n\
         {block}",
        block = bash_prompt_block(nonce)
    )
}

fn zsh_script(nonce: &Nonce) -> String {
    format!(
        "# Navo terminal integration (temporary ZDOTDIR; removed after the first prompt).\n\
         if [ -n \"${{__NAVO_ZDOTDIR:-}}\" ]; then\n\
           ZDOTDIR=\"$__NAVO_ZDOTDIR\"\n\
         else\n\
           unset ZDOTDIR\n\
         fi\n\
         unset __NAVO_INIT __NAVO_ZDOTDIR\n\
         if [ -f \"${{ZDOTDIR:-$HOME}}/.zshrc\" ]; then . \"${{ZDOTDIR:-$HOME}}/.zshrc\"; fi\n\
         unset __navo_nonce\n\
         __navo_nonce='{nonce}'\n\
         __navo_prompt() {{ printf '\\033]133;A;navo=%s\\033\\\\' \"$__navo_nonce\"; }}\n\
         precmd_functions+=(__navo_prompt)\n",
        nonce = nonce.as_str()
    )
}

fn fish_script(nonce: &Nonce) -> String {
    format!(
        "# Navo terminal integration (temporary file; removed after the first prompt).\n\
         # fish reads the user's config.fish before --init-command runs, so it is already restored.\n\
         set -e __NAVO_INIT __NAVO_ZDOTDIR\n\
         set -e __navo_nonce\n\
         set -g __navo_nonce '{nonce}'\n\
         if functions -q fish_prompt\n\
             functions -c fish_prompt __navo_user_prompt\n\
         else\n\
             function __navo_user_prompt\n\
                 printf '%s> ' (prompt_pwd)\n\
             end\n\
         end\n\
         function fish_prompt\n\
             printf '\\033]133;A;navo=%s\\033\\\\\\\\' \"$__navo_nonce\"\n\
             __navo_user_prompt\n\
         end\n",
        nonce = nonce.as_str()
    )
}

/// Forwards one early zsh startup file: restore the original `ZDOTDIR`, source the user's
/// file, then point `ZDOTDIR` back at the temp folder so the next startup file is ours.
fn zsh_forward_script(name: &str) -> String {
    format!(
        "# Navo terminal integration (temporary ZDOTDIR; removed after the first prompt).\n\
         __navo_zdotdir=\"$ZDOTDIR\"\n\
         if [ -n \"${{__NAVO_ZDOTDIR:-}}\" ]; then\n\
           ZDOTDIR=\"$__NAVO_ZDOTDIR\"\n\
         else\n\
           unset ZDOTDIR\n\
         fi\n\
         if [ -f \"${{ZDOTDIR:-$HOME}}/{name}\" ]; then . \"${{ZDOTDIR:-$HOME}}/{name}\"; fi\n\
         ZDOTDIR=\"$__navo_zdotdir\"\n\
         unset __navo_zdotdir\n"
    )
}

/// Creates the owner-only temp init, or logs and fails closed (no integration).
fn temp_init(files: &[(&str, &str)]) -> Option<Arc<TempInit>> {
    match TempInit::new(files) {
        Ok(init) => Some(Arc::new(init)),
        Err(error) => {
            tracing::warn!(%error, "shell prompt hook temp file failed");
            None
        }
    }
}

/// A temp path usable as shell text: UTF-8, non-empty and free of control characters.
fn clean_shell_path(path: &Path) -> Option<&str> {
    let text = path.to_str()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        None
    } else {
        Some(text)
    }
}

fn init_file_env(init: &TempInit) -> Option<String> {
    Some(clean_shell_path(init.file())?.to_owned())
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|arg| flags.contains(&arg.as_str()))
}
