use super::{Input, put_f32, put_str, put_u32};
use crate::protocol::{
    CharBox, CodecError, LinkTarget, MAX_PAGE_LINKS, MAX_URI_BYTES, PageLink, PageLinks,
};

pub(super) fn put_links(out: &mut Vec<u8>, links: &PageLinks) -> Result<(), CodecError> {
    out.push(7);
    put_f32(out, links.width());
    put_f32(out, links.height());
    put_u32(out, links.links().len() as u32);
    for link in links.links() {
        let bounds = link.bounds;
        for value in [bounds.left, bounds.top, bounds.right, bounds.bottom] {
            put_f32(out, value);
        }
        match &link.target {
            LinkTarget::Page { page, top } => {
                out.push(1);
                put_u32(out, *page);
                match top {
                    None => out.push(0),
                    Some(top) => {
                        out.push(1);
                        put_f32(out, *top);
                    }
                }
            }
            LinkTarget::Uri(uri) => {
                out.push(2);
                put_str(out, "URI", uri, MAX_URI_BYTES)?;
            }
            LinkTarget::Unsupported => out.push(3),
        }
    }
    Ok(())
}

pub(super) fn links(input: &mut Input<'_>) -> Result<PageLinks, CodecError> {
    let width = input.f32()?;
    let height = input.f32()?;
    let count = input.u32()? as usize;
    if count > MAX_PAGE_LINKS {
        return Err(CodecError::TooLarge {
            what: "links",
            len: count as u64,
            max: MAX_PAGE_LINKS as u64,
        });
    }
    let links = (0..count)
        .map(|_| page_link(input))
        .collect::<Result<Vec<_>, _>>()?;
    PageLinks::new(width, height, links)
}

fn page_link(input: &mut Input<'_>) -> Result<PageLink, CodecError> {
    let bounds = CharBox {
        left: input.f32()?,
        top: input.f32()?,
        right: input.f32()?,
        bottom: input.f32()?,
    };
    let target = match input.u8()? {
        1 => LinkTarget::Page {
            page: input.u32()?,
            top: match input.u8()? {
                0 => None,
                1 => Some(input.f32()?),
                _ => return Err(CodecError::Malformed("unknown option tag")),
            },
        },
        2 => LinkTarget::Uri(input.string("URI", MAX_URI_BYTES)?),
        3 => LinkTarget::Unsupported,
        _ => return Err(CodecError::Malformed("unknown link target")),
    };
    Ok(PageLink { bounds, target })
}
