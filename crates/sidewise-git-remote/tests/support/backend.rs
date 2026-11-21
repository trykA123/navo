//! Test-only `Http` backend: runs `git http-backend` as a CGI once per request (Linux tests).

use std::io::{self, BufRead, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use gix::error::Error;
use gix_transport::client::AuthenticationRequired;
use gix_transport::client::blocking_io::http::{GetResponse, Http, PostBodyDataKind, PostResponse};
use sidewise_git_remote::HttpStatus;

/// What a request path asks the backend to do before the CGI runs.
enum Mode {
    /// Serve the repository as `git http-backend` answers.
    Normal,
    /// Serve it, but cut a POST response body in half.
    Truncated,
    /// Answer 401.
    Private,
    /// Answer 302.
    Moved,
}

/// A parsed request: the mode, the CGI path info and the query string.
struct Route {
    mode: Mode,
    path_info: String,
    query: String,
}

/// Splits `url` into a mode prefix (`/private/`, `/moved/`, `/truncated/`), path and query.
fn route(url: &str) -> Route {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path_and_query = rest.find('/').map_or("/", |i| &rest[i..]);
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    let (mode, path) = match path.split_once('/').and_then(|(_, r)| r.split_once('/')) {
        Some(("private", _)) => (Mode::Private, path.to_owned()),
        Some(("moved", _)) => (Mode::Moved, path.to_owned()),
        Some(("truncated", rest)) => (Mode::Truncated, format!("/{rest}")),
        _ => (Mode::Normal, path.to_owned()),
    };
    Route {
        mode,
        path_info: path,
        query: query.to_owned(),
    }
}

/// A finished CGI answer: response header lines (without the status) and the body.
struct Answer {
    headers: Vec<u8>,
    body: Vec<u8>,
}

/// Maps a non-2xx status to the `io::Error` the `Http` trait asks for.
fn status_error(status: u16) -> io::Error {
    if status == 401 {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            AuthenticationRequired::default(),
        )
    } else {
        io::Error::other(HttpStatus(status))
    }
}

/// One request to serve: the repository folder, HTTP method, URL, header lines and body.
struct Request<'a> {
    root: &'a Path,
    method: &'a str,
    url: &'a str,
    extra: &'a [String],
    input: &'a [u8],
}

/// Runs the CGI for one request and returns its answer, or the status error.
fn run_cgi(req: &Request, route: &Route) -> io::Result<Answer> {
    let mut command = Command::new("git");
    command.arg("http-backend").env_clear();
    command
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_PROJECT_ROOT", req.root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("REQUEST_METHOD", req.method)
        .env("PATH_INFO", &route.path_info)
        .env("QUERY_STRING", &route.query)
        .env("CONTENT_LENGTH", req.input.len().to_string())
        .env("REMOTE_ADDR", "127.0.0.1");
    for line in req.extra {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "content-type" => command.env("CONTENT_TYPE", value.trim()),
            "git-protocol" => command.env("GIT_PROTOCOL", value.trim()),
            _ => &mut command,
        };
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("no stdin"))?;
    let input = req.input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output()?;
    writer
        .join()
        .map_err(|_| io::Error::other("stdin writer panicked"))?
        .ok();
    parse_cgi(&output.stdout)
}

/// Splits CGI output into headers and body; a `Status:` header above 299 becomes an error.
fn parse_cgi(out: &[u8]) -> io::Result<Answer> {
    let split = out
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| io::Error::other("bad CGI output"))?;
    let head = &out[..split + 2];
    let status = String::from_utf8_lossy(head)
        .lines()
        .find_map(|l| {
            l.strip_prefix("Status:")
                .and_then(|v| v.trim().split(' ').next()?.parse::<u16>().ok())
        })
        .unwrap_or(200);
    if status > 299 {
        return Err(status_error(status));
    }
    Ok(Answer {
        headers: head.to_vec(),
        body: out[split + 4..].to_vec(),
    })
}

/// A test `Http` implementation serving the bare repositories under one folder.
pub struct CgiHttp {
    root: PathBuf,
}

impl CgiHttp {
    /// Serves the bare repositories found directly under `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

/// Runs one request: answers the special modes, otherwise runs the CGI.
fn serve(req: &Request) -> io::Result<Answer> {
    let route = route(req.url);
    match route.mode {
        Mode::Private => return Err(status_error(401)),
        Mode::Moved => return Err(status_error(302)),
        Mode::Normal | Mode::Truncated => {}
    }
    let repo = route
        .path_info
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("");
    if repo.is_empty() || !req.root.join(repo).is_dir() {
        return Err(status_error(404));
    }
    let mut answer = run_cgi(req, &route)?;
    if matches!(route.mode, Mode::Truncated) && req.method == "POST" {
        answer.body.truncate(answer.body.len() / 2);
    }
    Ok(answer)
}

/// The request state shared by a POST's writer, headers and body.
struct Pending {
    root: PathBuf,
    url: String,
    extra: Vec<String>,
    input: Vec<u8>,
    answer: Option<io::Result<Answer>>,
}

/// Shared handle to a POST that has not run yet.
type Shared = Arc<Mutex<Pending>>;

/// Which half of an answer a reader exposes.
#[derive(Clone, Copy)]
enum Part {
    Headers,
    Body,
}

/// A reader over one part of an answer; for a POST the CGI runs on its first read.
pub struct LazyRead {
    shared: Shared,
    part: Part,
    data: Option<Cursor<Vec<u8>>>,
}

impl LazyRead {
    /// A reader over bytes that already exist (a finished GET).
    fn ready(bytes: Vec<u8>) -> Self {
        let empty = Pending {
            root: PathBuf::new(),
            url: String::new(),
            extra: Vec::new(),
            input: Vec::new(),
            answer: None,
        };
        Self {
            shared: Arc::new(Mutex::new(empty)),
            part: Part::Body,
            data: Some(Cursor::new(bytes)),
        }
    }

    /// Runs the CGI if no reader did yet, then takes this reader's part (or the stored error).
    fn ensure(&mut self) -> io::Result<&mut Cursor<Vec<u8>>> {
        if self.data.is_none() {
            let mut state = self
                .shared
                .lock()
                .map_err(|_| io::Error::other("poisoned"))?;
            if state.answer.is_none() {
                let result = serve(&Request {
                    root: &state.root,
                    method: "POST",
                    url: &state.url,
                    extra: &state.extra,
                    input: &state.input,
                });
                state.answer = Some(result);
            }
            let answer = state
                .answer
                .as_mut()
                .ok_or_else(|| io::Error::other("no answer"))?;
            let part = match answer {
                Ok(a) if matches!(self.part, Part::Headers) => std::mem::take(&mut a.headers),
                Ok(a) => std::mem::take(&mut a.body),
                Err(err) => {
                    return Err(std::mem::replace(
                        err,
                        io::Error::other("error already reported"),
                    ));
                }
            };
            self.data = Some(Cursor::new(part));
        }
        self.data
            .as_mut()
            .ok_or_else(|| io::Error::other("no data"))
    }
}

impl Read for LazyRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.ensure()?.read(buf)
    }
}

impl BufRead for LazyRead {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.ensure()?.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        if let Some(data) = self.data.as_mut() {
            data.consume(amt);
        }
    }
}

/// The write side of a POST: buffers the request body until the response is read.
pub struct PostBody {
    shared: Shared,
}

impl Write for PostBody {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self
            .shared
            .lock()
            .map_err(|_| io::Error::other("poisoned"))?;
        state.input.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Http for CgiHttp {
    type Headers = LazyRead;
    type ResponseBody = LazyRead;
    type PostBody = PostBody;

    fn get(
        &mut self,
        url: &str,
        _base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::error::Result<GetResponse<Self::Headers, Self::ResponseBody>> {
        let extra: Vec<String> = headers.into_iter().map(|h| h.as_ref().to_owned()).collect();
        let answer = serve(&Request {
            root: &self.root,
            method: "GET",
            url,
            extra: &extra,
            input: &[],
        })
        .map_err(Error::from_error)?;
        Ok(GetResponse {
            headers: LazyRead::ready(answer.headers),
            body: LazyRead::ready(answer.body),
        })
    }

    fn post(
        &mut self,
        url: &str,
        _base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        _body: PostBodyDataKind,
    ) -> gix::error::Result<PostResponse<Self::Headers, Self::ResponseBody, Self::PostBody>> {
        let extra: Vec<String> = headers.into_iter().map(|h| h.as_ref().to_owned()).collect();
        let shared = Arc::new(Mutex::new(Pending {
            root: self.root.clone(),
            url: url.to_owned(),
            extra,
            input: Vec::new(),
            answer: None,
        }));
        let lazy = |part| LazyRead {
            shared: shared.clone(),
            part,
            data: None,
        };
        Ok(PostResponse {
            post_body: PostBody {
                shared: shared.clone(),
            },
            headers: lazy(Part::Headers),
            body: lazy(Part::Body),
        })
    }

    fn configure(&mut self, _config: &dyn std::any::Any) -> gix::error::Result {
        Ok(())
    }
}
