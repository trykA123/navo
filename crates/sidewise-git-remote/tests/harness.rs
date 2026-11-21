//! Smoke tests for the test-only `git http-backend` transport (Linux, medium size).
// Runs `git http-backend` as a CGI, which only the Linux test host provides (NC226).
#![cfg(not(windows))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support: a failed setup step should abort the test"
)]

mod support;

use std::io::{BufRead, Read};

use gix_transport::client::AuthenticationRequired;
use gix_transport::client::blocking_io::http::{Http, PostBodyDataKind};
use sidewise_git_remote::HttpStatus;
use support::{CgiHttp, fixture};

const BASE: &str = "http://test.invalid";
const UPLOAD_PACK: &str = "git-upload-pack";

/// Runs `GET info/refs` for `path` and returns the response body text.
fn advertise(path: &str) -> Result<String, gix::error::Error> {
    let mut http = CgiHttp::new(fixture().root());
    let base = format!("{BASE}/{path}");
    let url = format!("{base}/info/refs?service={UPLOAD_PACK}");
    let response = http.get(&url, &base, ["Git-Protocol: version=1"])?;
    let mut text = String::new();
    response
        .body
        .take(1 << 20)
        .read_to_string(&mut text)
        .map_err(gix::error::Error::from_error)?;
    Ok(text)
}

/// Finds the `io::Error` inside a gix error.
fn io_error(err: &gix::error::Error) -> &std::io::Error {
    err.downcast_any_ref::<std::io::Error>()
        .expect("an io::Error")
}

#[test]
fn info_refs_lists_main() {
    let text = advertise("repo.git").unwrap();
    assert!(text.contains("refs/heads/main"), "{text}");
    assert!(text.contains("refs/tags/v1"), "{text}");
    assert!(text.contains("refs/pull/1/head"), "{text}");
}

#[test]
fn private_is_401_permission_denied() {
    let err = advertise("private/repo.git").unwrap_err();
    let io = io_error(&err);
    assert_eq!(io.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(io.get_ref().unwrap().is::<AuthenticationRequired>());
}

#[test]
fn unknown_repo_is_404() {
    let err = advertise("missing.git").unwrap_err();
    let status = io_error(&err)
        .get_ref()
        .unwrap()
        .downcast_ref::<HttpStatus>();
    assert_eq!(status, Some(&HttpStatus(404)));
}

#[test]
fn moved_is_302() {
    let err = advertise("moved/repo.git").unwrap_err();
    let status = io_error(&err)
        .get_ref()
        .unwrap()
        .downcast_ref::<HttpStatus>();
    assert_eq!(status, Some(&HttpStatus(302)));
}

/// Posts a protocol v2 `fetch` of the main commit and returns the response bytes.
fn fetch_main(path: &str) -> Vec<u8> {
    let fixture = fixture();
    let mut http = CgiHttp::new(fixture.root());
    let base = format!("{BASE}/{path}");
    let url = format!("{base}/{UPLOAD_PACK}");
    let headers = [
        "Git-Protocol: version=2",
        "Content-Type: application/x-git-upload-pack-request",
    ];
    let mut response = http
        .post(
            &url,
            &base,
            headers,
            PostBodyDataKind::BoundedAndFitsIntoMemory,
        )
        .unwrap();
    std::io::Write::write_all(
        &mut response.post_body,
        &support::fetch_request(fixture.main_oid()),
    )
    .unwrap();
    drop(response.post_body);
    let mut headers = String::new();
    response.headers.read_line(&mut headers).unwrap();
    let mut bytes = Vec::new();
    response.body.read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn truncated_cuts_the_pack_in_half() {
    let full = fetch_main("repo.git");
    let cut = fetch_main("truncated/repo.git");
    assert!(full.len() > 1 << 20, "full pack is {} bytes", full.len());
    // Two runs build packs of slightly different size, so compare with a 1% margin.
    assert!(
        cut.len().abs_diff(full.len() / 2) < full.len() / 100,
        "cut {} of {}",
        cut.len(),
        full.len()
    );
}

#[test]
fn gix_handshake_reads_v2_capabilities() {
    use gix_transport::Protocol;
    use gix_transport::client::blocking_io::Transport as _;
    use gix_transport::client::blocking_io::http::connect_http;

    let url = gix::url::parse(gix::bstr::BStr::new(&format!("{BASE}/repo.git"))).unwrap();
    let mut transport = connect_http(CgiHttp::new(fixture().root()), url, Protocol::V2, false);
    let handshake = transport
        .handshake(gix_transport::Service::UploadPack, &[])
        .unwrap();
    assert!(handshake.capabilities.contains("ls-refs"));
}
