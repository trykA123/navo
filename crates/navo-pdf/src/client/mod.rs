mod helper;
mod pool;
mod process;
mod reaper;
mod share;
mod spawn;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use helper::Helper;
use pool::Pool;
pub use process::{HelperProcess, Spawned};
use share::Share;
#[cfg(windows)]
pub use share::ShareFile;
#[cfg(not(windows))]
pub use share::TestShare;
pub use spawn::{HelperSpec, Permanence, Spawn, Unconfined};

use crate::protocol::{
    Bitmap, OpenSource, Outline, PageLinks, PageText, PdfError, RenderRequest, Request, Response,
};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

const WARM_WAIT: Duration = Duration::from_secs(2);
const SHUTDOWN_WAIT: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct Launcher {
    spec: HelperSpec,
    timeout: Duration,
    warm_wait: Duration,
    spawn: Option<Spawn>,
    share: Option<Share>,
    pool: Arc<Pool<Helper>>,
    shutdowns: Arc<AtomicU64>,
    /// Set by the first permanent `Unconfined` refusal; from then on no helper is spawned.
    unconfinable: Arc<OnceLock<String>>,
}

impl Launcher {
    pub fn new(program: impl Into<PathBuf>, args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            spec: HelperSpec {
                program: program.into(),
                args: args.into_iter().collect(),
            },
            timeout: DEFAULT_TIMEOUT,
            warm_wait: WARM_WAIT,
            spawn: None,
            share: None,
            pool: Arc::default(),
            shutdowns: Arc::default(),
            unconfinable: Arc::default(),
        }
    }

    pub fn prewarm(&self) -> Result<(), PdfError> {
        let born = self.shutdowns.load(Ordering::SeqCst);
        self.pool.warm(|| {
            let mut helper = Helper::spawn(self, born)?;
            helper.prepare(self.timeout)?;
            self.check_generation(born)?;
            Ok(helper)
        })
    }

    fn helper(&self) -> Result<Helper, PdfError> {
        let born = self.shutdowns.load(Ordering::SeqCst);
        if let Some(mut warm) = self.pool.take(self.warm_wait)
            && warm.prepare(self.timeout).is_ok()
        {
            self.check_generation(born)?;
            return Ok(warm);
        }
        Helper::spawn(self, born)
    }

    fn check_generation(&self, born: u64) -> Result<(), PdfError> {
        if self.shutdowns.load(Ordering::SeqCst) != born {
            return Err(PdfError::HelperDied);
        }
        Ok(())
    }

    pub(crate) fn wait_for_idle(&self, wait: Duration) -> bool {
        self.pool.wait_for_idle(wait)
    }

    /// Stops every pooled and recycling helper and returns once they have exited. A helper still
    /// running after `SHUTDOWN_WAIT` is killed again and waited for, at most `KILL_WAIT` more.
    pub fn shutdown(&self) {
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        self.pool.close();
        let finished = reaper::shutdown(self.shutdowns.clone(), deadline);
        self.pool
            .settle(deadline.saturating_duration_since(Instant::now()));
        if let Some(finished) = finished {
            let limit = deadline + reaper::KILL_WAIT;
            let _ = finished.recv_timeout(limit.saturating_duration_since(Instant::now()));
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[doc(hidden)]
    #[must_use]
    pub fn with_warm_wait(mut self, wait: Duration) -> Self {
        self.warm_wait = wait;
        self
    }

    /// Starts every helper through `spawn` instead of the default `Command` spawn.
    #[must_use]
    pub fn with_spawn(mut self, spawn: Spawn) -> Self {
        self.spawn = Some(spawn);
        self
    }
}

pub struct Document {
    launcher: Launcher,
    path: String,
    pages: u32,
    helper: Option<Helper>,
}

impl Document {
    pub fn open(
        launcher: &Launcher,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Self, PdfError> {
        let path = path
            .to_str()
            .ok_or_else(|| PdfError::Unreadable("the path is not valid Unicode".into()))?;
        let mut document = Self {
            launcher: launcher.clone(),
            path: path.to_owned(),
            pages: 0,
            helper: None,
        };
        document.start(password)?;
        Ok(document)
    }

    pub fn pages(&self) -> u32 {
        self.pages
    }

    pub fn page_count(&mut self) -> Result<u32, PdfError> {
        match self.call(&Request::PageCount)? {
            Response::PageCount(count) => Ok(count),
            other => Err(self.unexpected(&other)),
        }
    }

    pub fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError> {
        match self.call(&Request::Render(request))? {
            Response::Page(bitmap) => Ok(bitmap),
            other => Err(self.unexpected(&other)),
        }
    }

    pub fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        match self.call(&Request::Text { page })? {
            Response::Text(text) => Ok(text),
            other => Err(self.unexpected(&other)),
        }
    }

    pub fn links(&mut self, page: u32) -> Result<PageLinks, PdfError> {
        match self.call(&Request::Links { page })? {
            Response::Links(links) => Ok(links),
            other => Err(self.unexpected(&other)),
        }
    }

    pub fn outline(&mut self) -> Result<Outline, PdfError> {
        match self.call(&Request::Outline)? {
            Response::Outline(outline) => Ok(outline),
            other => Err(self.unexpected(&other)),
        }
    }

    pub fn close(self) {}

    /// Opens the document in a helper: with a share hook it opens and size-checks the file first,
    /// then shares a handle; any failure drops the helper (disposed, never pooled) and the `File`.
    fn start(&mut self, password: Option<&str>) -> Result<&mut Helper, PdfError> {
        let hook = self.launcher.share.as_ref();
        let file = hook.map(|_| share::open_checked(&self.path)).transpose()?;
        let mut helper = self.launcher.helper()?;
        let source = match (hook, &file) {
            (Some(hook), Some(file)) => OpenSource::Handle(share::share_into(hook, file, &helper)?),
            _ => OpenSource::Path(self.path.clone()),
        };
        let open = Request::Open {
            source,
            password: password.map(str::to_owned),
        };
        match helper.call(&open, self.launcher.timeout)? {
            Response::Opened { pages } => self.pages = pages,
            Response::Failed(error) => return Err(error),
            other => return Err(unexpected_reply(&other)),
        }
        Ok(self.helper.insert(helper))
    }

    fn call(&mut self, request: &Request) -> Result<Response, PdfError> {
        let timeout = self.launcher.timeout;
        let helper = match self.helper.take() {
            Some(helper) => self.helper.insert(helper),
            None => self.start(None)?,
        };
        match helper.call(request, timeout) {
            Ok(Response::Failed(error)) => Err(error),
            Ok(response) => Ok(response),
            Err(error) => {
                self.helper = None;
                Err(error)
            }
        }
    }

    fn unexpected(&mut self, response: &Response) -> PdfError {
        self.helper = None;
        unexpected_reply(response)
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        if let Some(helper) = self.helper.take() {
            reaper::recycle(helper, self.launcher.timeout, self.launcher.pool.clone());
        }
    }
}

fn unexpected_reply(response: &Response) -> PdfError {
    let name = match response {
        Response::Opened { .. } => "Opened",
        Response::PageCount(_) => "PageCount",
        Response::Page(_) => "Page",
        Response::Text(_) => "Text",
        Response::Links(_) => "Links",
        Response::Outline(_) => "Outline",
        Response::Closed => "Closed",
        Response::Failed(_) => "Failed",
    };
    PdfError::Protocol(format!("unexpected {name} reply"))
}
