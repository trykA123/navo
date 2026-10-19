use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Error;
use crate::cli::Start;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub(crate) path: Option<PathBuf>,
    pub(crate) left: Option<PathBuf>,
    pub(crate) right: Option<PathBuf>,
    pub(crate) view: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Versioned {
    v: u8,
    #[serde(flatten)]
    request: Request,
}

impl Request {
    pub(crate) fn from_start(start: &Start, cwd: &Path) -> Result<Self, Error> {
        let resolve = |path: &Option<PathBuf>| {
            path.as_ref()
                .map(|path| std::path::absolute(cwd.join(path)))
                .transpose()
        };
        let request = Self {
            path: resolve(&start.path)?,
            left: resolve(&start.left)?,
            right: resolve(&start.right)?,
            view: resolve(&start.view)?,
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn parse(line: &str) -> Result<Self, Error> {
        let envelope: Versioned = serde_json::from_str(line)?;
        if envelope.v != 1 {
            return Err(Error::InvalidRequest);
        }
        envelope.request.validate()?;
        Ok(envelope.request)
    }

    pub(crate) fn to_line(&self) -> Result<String, Error> {
        self.validate()?;
        #[derive(Serialize)]
        struct Envelope<'a> {
            v: u8,
            #[serde(flatten)]
            request: &'a Request,
        }
        let mut line = serde_json::to_string(&Envelope {
            v: 1,
            request: self,
        })?;
        line.push('\n');
        Ok(line)
    }

    fn validate(&self) -> Result<(), Error> {
        if [&self.path, &self.left, &self.right, &self.view]
            .into_iter()
            .flatten()
            .any(|path| !path.is_absolute())
        {
            return Err(Error::InvalidRequest);
        }
        Ok(())
    }
}
