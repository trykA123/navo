//! Sidewise's gix HTTP backend on Windows (NC217, R3): each smart-HTTP request is one
//! `sidewise_winhttp::Exchange`. A POST body is buffered and sent on the first response read, and
//! every failure surfaces while the headers are read, as gix's curl and reqwest backends do.

use std::cell::RefCell;
use std::io::{self, BufReader, Cursor, Read, Write};
use std::rc::Rc;

use gix::bstr::BString;
use gix_transport::client::AuthenticationRequired;
use gix_transport::client::blocking_io::http::{GetResponse, Http, PostBodyDataKind, PostResponse};
use sidewise_winhttp::{Exchange, Target, Verb, WinHttpError};

use crate::headers::{header_block, split_url};
use crate::{HttpStatus, NetworkFailure};

/// The largest POST body buffered for one request, in bytes (R3).
const MAX_POST_BYTES: usize = 1024 * 1024;
/// WinHTTP resolve, connect, send and receive timeouts in milliseconds (NC221).
const TIMEOUTS_MS: [i32; 4] = [10_000, 10_000, 60_000, 60_000];

/// The WinHTTP backend; it keeps no state between requests.
#[derive(Debug, Default)]
pub struct WinHttp;

impl WinHttp {
    /// A backend with the product rules: user agent `Sidewise` and the NC221 timeouts.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// One request, shared by its post-body writer and its two readers on one thread.
type Shared = Rc<RefCell<Call>>;

/// An exchange with its buffered post body and how far it got.
struct Call {
    exchange: Exchange,
    post: Vec<u8>,
    state: State,
}

/// Whether the request went out; a failed send is kept so every later read fails the same way.
#[derive(Clone, Copy)]
enum State {
    Unsent,
    Sent,
    Failed(WinHttpError),
}

impl Call {
    /// Sends the request with the buffered body once, then checks the status: 2xx is `Ok`, 401 is
    /// `PermissionDenied` with `AuthenticationRequired`, others `HttpStatus`, WinHTTP `NetworkFailure`.
    fn answered(&mut self) -> io::Result<()> {
        if matches!(self.state, State::Unsent) {
            let body = std::mem::take(&mut self.post);
            self.state = self
                .exchange
                .send(&body)
                .map_or_else(State::Failed, |()| State::Sent);
        }
        if let State::Failed(err) = self.state {
            return Err(io_error(err));
        }
        match self.exchange.status() {
            200..=299 => Ok(()),
            401 => {
                let www_authenticate = challenges(&self.exchange);
                let auth = AuthenticationRequired { www_authenticate };
                Err(io::Error::new(io::ErrorKind::PermissionDenied, auth))
            }
            status => Err(io::Error::other(HttpStatus(status))),
        }
    }
}

/// The `WWW-Authenticate` values of the response, in order; none when the headers cannot be read.
fn challenges(exchange: &Exchange) -> Vec<BString> {
    let text = exchange.raw_headers().unwrap_or_default();
    let values = text.lines().filter_map(|line| line.split_once(':'));
    let auth = values.filter(|(name, _)| name.trim().eq_ignore_ascii_case("www-authenticate"));
    auth.map(|(_, value)| BString::from(value.trim())).collect()
}

/// Maps a WinHTTP failure to the `io::Error` that `problem_of` reads.
fn io_error(err: WinHttpError) -> io::Error {
    match err {
        WinHttpError::Network(code) => io::Error::other(NetworkFailure(code)),
        WinHttpError::Status(status) => io::Error::other(HttpStatus(status)),
        WinHttpError::TooLarge => io::Error::new(io::ErrorKind::InvalidData, "WinHTTP body cap"),
    }
}

/// The response headers as text; the first read sends the request and raises its failure.
pub struct HeaderText {
    call: Shared,
    text: Option<Cursor<Vec<u8>>>,
}

impl Read for HeaderText {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.text.is_none() {
            let mut call = self.call.borrow_mut();
            call.answered()?;
            let text = call.exchange.raw_headers().map_err(io_error)?;
            self.text = Some(Cursor::new(text.into_bytes()));
        }
        self.text.as_mut().map_or(Ok(0), |text| text.read(buf))
    }
}

/// The response body, streamed from WinHTTP; it fails while the status is not 2xx.
pub struct BodyBytes(Shared);

impl Read for BodyBytes {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut call = self.0.borrow_mut();
        call.answered()?;
        call.exchange.read(buf).map_err(io_error)
    }
}

/// The POST body buffer: at most `MAX_POST_BYTES`, and only before the request is sent.
pub struct PostBody(Shared);

impl Write for PostBody {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut call = self.0.borrow_mut();
        if !matches!(call.state, State::Unsent) {
            return Err(io::Error::other(
                "post body written after the request was sent",
            ));
        }
        if call.post.len().saturating_add(buf.len()) > MAX_POST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "post body over 1 MiB",
            ));
        }
        call.post.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Opens `verb url` with gix's header lines minus `User-Agent`; nothing is sent yet. Header lines
/// with CR or LF fail.
fn open(
    url: &str,
    verb: Verb,
    headers: impl IntoIterator<Item = impl AsRef<str>>,
) -> gix::Result<Shared> {
    let (host, port, path) = split_url(url).map_err(gix::Error::from_error)?;
    let block = header_block(headers).map_err(gix::Error::from_error)?;
    let target = Target {
        host,
        port,
        user_agent: "Sidewise",
        timeouts_ms: TIMEOUTS_MS,
    };
    let exchange = Exchange::open(&target, verb, path, &block)
        .map_err(|err| gix::Error::from_error(io_error(err)))?;
    let state = State::Unsent;
    Ok(Rc::new(RefCell::new(Call {
        exchange,
        post: Vec::new(),
        state,
    })))
}

/// The post-body writer and both readers of one open request.
fn response(call: Shared) -> PostResponse<BufReader<HeaderText>, BufReader<BodyBytes>, PostBody> {
    let headers = HeaderText {
        call: Rc::clone(&call),
        text: None,
    };
    PostResponse {
        post_body: PostBody(Rc::clone(&call)),
        headers: BufReader::new(headers),
        body: BufReader::new(BodyBytes(call)),
    }
}

impl Http for WinHttp {
    type Headers = BufReader<HeaderText>;
    type ResponseBody = BufReader<BodyBytes>;
    type PostBody = PostBody;

    fn get(
        &mut self,
        url: &str,
        _base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::Result<GetResponse<Self::Headers, Self::ResponseBody>> {
        Ok(response(open(url, Verb::Get, headers)?).into())
    }

    fn post(
        &mut self,
        url: &str,
        _base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        _body: PostBodyDataKind,
    ) -> gix::Result<PostResponse<Self::Headers, Self::ResponseBody, Self::PostBody>> {
        Ok(response(open(url, Verb::Post, headers)?))
    }

    /// No options: with no gix backend feature, gix passes none (NC217).
    fn configure(&mut self, _config: &dyn std::any::Any) -> gix::Result {
        Ok(())
    }
}
