//! One test per row of IW33's start table, plus the same-SID other-session and foreign-shell cases.

use super::*;

const USER: &str = "S-1-5-21-1000-2000-3000-1001";
const OTHER: &str = "S-1-5-21-1000-2000-3000-1002";

/// Builds Setup's own token with a readable SID in session 1.
fn own(elevated: Result<bool, ()>) -> OwnToken {
    OwnToken {
        elevated,
        sid: Some(USER.into()),
        session: 1,
    }
}

/// Builds a seen shell with the given SID and session.
fn shell(sid: &str, session: u32) -> ShellSeen {
    ShellSeen::Facts(ShellFacts {
        sid: sid.into(),
        session,
    })
}

#[test]
fn an_elevated_token_whose_shell_has_the_same_sid_and_session_is_as_shell_user() {
    assert_eq!(
        classify(&own(Ok(true)), &shell(USER, 1)),
        StartMode::AsShellUser
    );
}

#[test]
fn the_same_sid_in_another_case_is_the_same_user() {
    assert_eq!(
        classify(&own(Ok(true)), &shell(&USER.to_ascii_lowercase(), 1)),
        StartMode::AsShellUser
    );
}

#[test]
fn an_unelevated_token_is_normal_whatever_the_shell() {
    for seen in [
        ShellSeen::Missing,
        ShellSeen::Foreign,
        shell(USER, 1),
        shell(OTHER, 2),
    ] {
        assert_eq!(classify(&own(Ok(false)), &seen), StartMode::Normal);
    }
}

#[test]
fn an_unelevated_token_without_a_sid_is_still_normal() {
    let token = OwnToken {
        sid: None,
        ..own(Ok(false))
    };
    assert_eq!(classify(&token, &ShellSeen::Missing), StartMode::Normal);
}

#[test]
fn an_elevated_token_whose_shell_is_another_user_is_other_account() {
    assert_eq!(
        classify(&own(Ok(true)), &shell(OTHER, 1)),
        StartMode::Refused(StartRefusal::OtherAccount)
    );
}

#[test]
fn an_elevated_token_whose_shell_cannot_be_opened_is_other_account() {
    assert_eq!(
        classify(&own(Ok(true)), &ShellSeen::Foreign),
        StartMode::Refused(StartRefusal::OtherAccount)
    );
}

#[test]
fn an_elevated_token_with_no_shell_is_no_desktop() {
    assert_eq!(
        classify(&own(Ok(true)), &ShellSeen::Missing),
        StartMode::Refused(StartRefusal::NoDesktop)
    );
}

#[test]
fn an_elevated_token_whose_shell_has_the_same_sid_in_another_session_is_no_desktop() {
    assert_eq!(
        classify(&own(Ok(true)), &shell(USER, 2)),
        StartMode::Refused(StartRefusal::NoDesktop)
    );
}

#[test]
fn an_unreadable_elevation_is_unknown_whatever_the_shell() {
    for seen in [
        ShellSeen::Missing,
        ShellSeen::Foreign,
        shell(USER, 1),
        shell(OTHER, 2),
    ] {
        assert_eq!(
            classify(&own(Err(())), &seen),
            StartMode::Refused(StartRefusal::Unknown)
        );
    }
}

#[test]
fn an_elevated_token_without_a_sid_is_unknown() {
    let token = OwnToken {
        sid: None,
        ..own(Ok(true))
    };
    assert_eq!(
        classify(&token, &shell(USER, 1)),
        StartMode::Refused(StartRefusal::Unknown)
    );
}
