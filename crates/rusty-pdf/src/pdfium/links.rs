use pdfium_render::prelude::{
    PdfAction, PdfDestination, PdfDestinationViewSettings as View, PdfPage, PdfRect,
};

use super::{PdfiumEngine, Upright, quarter_turns};
use crate::protocol::{
    CharBox, LinkTarget, MAX_PAGE_LINKS, MAX_URI_BYTES, PageLink, PageLinks, PdfError,
};

pub(super) fn extract(engine: &PdfiumEngine, page: PdfPage<'_>) -> Result<PageLinks, PdfError> {
    let (width, height) = (page.width().value, page.height().value);
    let upright = Upright::new(page.rotation().map_or(0, quarter_turns), width, height);
    let annotations = page.annotations();
    let mut links = Vec::new();
    let mut count = 0;
    for index in annotations.as_range() {
        let Ok(annotation) = annotations.get(index) else {
            continue;
        };
        let Some(annotation) = annotation.as_link_annotation() else {
            continue;
        };
        count += 1;
        if count > MAX_PAGE_LINKS {
            return Err(PdfError::TooLarge);
        }
        let Ok(link) = annotation.link() else {
            continue;
        };
        let Some(bounds) = link
            .rect()
            .ok()
            .and_then(|rect| link_bounds(&upright, rect))
        else {
            continue;
        };
        links.push(PageLink {
            bounds,
            target: action_target(engine, link.action(), || link.destination()),
        });
    }
    PageLinks::new(width, height, links).map_err(|_| PdfError::Corrupt)
}

pub(super) fn action_target<'a>(
    engine: &PdfiumEngine,
    action: Option<PdfAction<'a>>,
    destination: impl FnOnce() -> Option<PdfDestination<'a>>,
) -> LinkTarget {
    match action {
        Some(PdfAction::LocalDestination(action)) => action
            .destination()
            .map(|destination| destination_target(engine, destination))
            .unwrap_or(LinkTarget::Unsupported),
        Some(PdfAction::Uri(action)) => action
            .uri()
            .map(uri_target)
            .unwrap_or(LinkTarget::Unsupported),
        None => destination()
            .map(|destination| destination_target(engine, destination))
            .unwrap_or(LinkTarget::Unsupported),
        Some(_) => LinkTarget::Unsupported,
    }
}

fn destination_target(engine: &PdfiumEngine, destination: PdfDestination<'_>) -> LinkTarget {
    let Some(page) = destination
        .page_index()
        .ok()
        .and_then(|index| u32::try_from(index).ok())
    else {
        return LinkTarget::Unsupported;
    };
    let top = engine.page(page).ok().and_then(|page| {
        let upright = Upright::new(
            page.rotation().map_or(0, quarter_turns),
            page.width().value,
            page.height().value,
        );
        destination_top(&upright, destination.view_settings().ok()?)
    });
    LinkTarget::Page { page, top }
}

pub(super) fn link_bounds(upright: &Upright, rect: PdfRect) -> Option<CharBox> {
    let bounds = CharBox {
        left: rect.left().value,
        top: rect.top().value,
        right: rect.right().value,
        bottom: rect.bottom().value,
    };
    if !bounds.is_finite() {
        return None;
    }
    let bounds = upright.map((bounds.left, bounds.bottom), (bounds.right, bounds.top));
    bounds.is_finite().then_some(bounds)
}

pub(super) fn destination_top(upright: &Upright, view: View) -> Option<f32> {
    let (x, y) = match view {
        View::SpecificCoordinatesAndZoom(x, y, _) => (x, y),
        View::FitPageHorizontallyToWindow(y) | View::FitBoundsHorizontallyToWindow(y) => (None, y),
        View::FitPageVerticallyToWindow(x) | View::FitBoundsVerticallyToWindow(x) => (x, None),
        View::FitPageToRectangle(rect) => (Some(rect.left()), Some(rect.top())),
        _ => return None,
    };
    let point = if upright.turns % 2 == 1 {
        (x?.value, 0.0)
    } else {
        (0.0, y?.value)
    };
    let top = upright.point(point).1;
    top.is_finite().then_some(top)
}

pub(super) fn uri_target(uri: String) -> LinkTarget {
    if uri.len() > MAX_URI_BYTES {
        LinkTarget::Unsupported
    } else {
        LinkTarget::Uri(uri)
    }
}
