use super::{CharBox, CodecError, MAX_PAGE_LINKS, MAX_URI_BYTES};

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    Page { page: u32, top: Option<f32> },
    Uri(String),
    Unsupported,
}

impl LinkTarget {
    pub(super) fn validate(&self) -> Result<(), CodecError> {
        match self {
            Self::Page { top: Some(top), .. } if !top.is_finite() => {
                Err(CodecError::Malformed("link destination is not finite"))
            }
            Self::Uri(uri) if uri.len() > MAX_URI_BYTES => Err(CodecError::TooLarge {
                what: "URI",
                len: uri.len() as u64,
                max: MAX_URI_BYTES as u64,
            }),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    pub bounds: CharBox,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLinks {
    width: f32,
    height: f32,
    links: Vec<PageLink>,
}

impl PageLinks {
    pub fn new(width: f32, height: f32, links: Vec<PageLink>) -> Result<Self, CodecError> {
        if !(width.is_finite() && height.is_finite()) {
            return Err(CodecError::Malformed("page size is not finite"));
        }
        if links.len() > MAX_PAGE_LINKS {
            return Err(CodecError::TooLarge {
                what: "links",
                len: links.len() as u64,
                max: MAX_PAGE_LINKS as u64,
            });
        }
        for link in &links {
            if !link.bounds.is_finite() {
                return Err(CodecError::Malformed("link box is not finite"));
            }
            link.target.validate()?;
        }
        Ok(Self {
            width,
            height,
            links,
        })
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn links(&self) -> &[PageLink] {
        &self.links
    }
}
