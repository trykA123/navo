mod helper;

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Arc;
use std::time::Duration;

use helper::Helper;

use crate::protocol::{
    Bitmap, Outline, PageLinks, PageText, PdfError, RenderRequest, Request, Response,
};

pub type Confine = Arc<dyn Fn(&Child) -> io::Result<Box<dyn Send>> + Send + Sync>;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Launcher {
    program: PathBuf,
    args: Vec<OsString>,
    timeout: Duration,
    confine: Option<Confine>,
}

impl Launcher {
    pub fn new(program: impl Into<PathBuf>, args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
            timeout: DEFAULT_TIMEOUT,
            confine: None,
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_confinement(mut self, confine: Confine) -> Self {
        self.confine = Some(confine);
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

    pub fn close(mut self) {
        if let Some(helper) = self.helper.as_mut() {
            let _ = helper.call(&Request::Close, self.launcher.timeout);
        }
    }

    fn start(&mut self, password: Option<&str>) -> Result<&mut Helper, PdfError> {
        let mut helper = Helper::spawn(&self.launcher)?;
        let open = Request::Open {
            path: self.path.clone(),
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
