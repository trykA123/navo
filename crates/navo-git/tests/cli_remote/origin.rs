use std::net::TcpListener;

use super::fixture::Fixture;

/// `main` tracks `origin` at `url`, without a fetch.
pub fn with_url_origin(fixture: &Fixture, url: &str) {
    fixture.git_ok(&["remote", "add", "origin", url]);
    fixture.git_ok(&["config", "branch.main.remote", "origin"]);
    fixture.git_ok(&["config", "branch.main.merge", "refs/heads/main"]);
}

/// A port nothing listens on.
pub fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
