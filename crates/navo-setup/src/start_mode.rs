//! IW33 start classifier: decides once, before any window or file work, whether Setup runs.
//! Pure: the probe reads the tokens; this file only applies the rule table.

#[cfg(test)]
#[path = "start_mode_tests.rs"]
mod tests;

/// How Setup may run, decided from its own token and the desktop shell's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    Normal,
    AsShellUser,
    Refused(StartRefusal),
}

/// Why an elevated start is refused; every refusal exits 7 in silent mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartRefusal {
    OtherAccount,
    NoDesktop,
    Unknown,
}

/// Setup's own token: elevation (`Err` on a read error), user SID and session id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnToken {
    pub elevated: Result<bool, ()>,
    pub sid: Option<String>,
    pub session: u32,
}

/// The desktop shell's user SID and session, read from the token of a held process handle (IW40).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellFacts {
    pub sid: String,
    pub session: u32,
}

/// What the shell probe saw: no shell, a shell Setup may not open, or its facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellSeen {
    Missing,
    Foreign,
    Facts(ShellFacts),
}

/// Applies IW33's table row by row; an unreadable own token (elevation or SID) is `Unknown`.
/// A `Normal` start never looks at the shell; the shell's own elevation never matters.
pub fn classify(own: &OwnToken, shell: &ShellSeen) -> StartMode {
    let Ok(elevated) = own.elevated else {
        return StartMode::Refused(StartRefusal::Unknown);
    };
    if !elevated {
        return StartMode::Normal;
    }
    let Some(own_sid) = own.sid.as_deref() else {
        return StartMode::Refused(StartRefusal::Unknown);
    };
    match shell {
        ShellSeen::Missing => StartMode::Refused(StartRefusal::NoDesktop),
        ShellSeen::Foreign => StartMode::Refused(StartRefusal::OtherAccount),
        ShellSeen::Facts(facts) if !facts.sid.eq_ignore_ascii_case(own_sid) => {
            StartMode::Refused(StartRefusal::OtherAccount)
        }
        ShellSeen::Facts(facts) if facts.session != own.session => {
            StartMode::Refused(StartRefusal::NoDesktop)
        }
        ShellSeen::Facts(_) => StartMode::AsShellUser,
    }
}
