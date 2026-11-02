use std::collections::HashMap;

use pdfium_render::prelude::{
    PdfAction, PdfDestination, PdfDestinationViewSettings as View, PdfPage, PdfRect,
};

use super::{PdfiumEngine, Upright, quarter_turns};
use crate::protocol::{
    CharBox, LinkTarget, MAX_PAGE_LINKS, MAX_URI_BYTES, PageLink, PageLinks, PdfError,
};

pub(super) struct Targets<F> {
    load: F,
    known: HashMap<u32, Option<Upright>>,
}

impl<F: FnMut(u32) -> Option<Upright>> Targets<F> {
    pub(super) fn new(load: F) -> Self {
        Self {
            load,
            known: HashMap::new(),
        }
    }

    pub(super) fn upright(&mut self, page: u32) -> Option<&Upright> {
        let load = &mut self.load;
        self.known
            .entry(page)
            .or_insert_with(|| load(page))
            .as_ref()
    }

    pub(super) fn target<'a>(
        &mut self,
        action: Option<PdfAction<'a>>,
        destination: impl FnOnce() -> Option<PdfDestination<'a>>,
    ) -> LinkTarget {
        match action {
            Some(PdfAction::LocalDestination(action)) => action
                .destination()
                .map(|destination| self.destination_target(&destination))
                .unwrap_or(LinkTarget::Unsupported),
            Some(PdfAction::Uri(action)) => action
                .uri()
                .map(uri_target)
                .unwrap_or(LinkTarget::Unsupported),
            None => destination()
                .map(|destination| self.destination_target(&destination))
                .unwrap_or(LinkTarget::Unsupported),
            Some(_) => LinkTarget::Unsupported,
        }
    }

    fn destination_target(&mut self, destination: &PdfDestination<'_>) -> LinkTarget {
        let Some(page) = destination
            .page_index()
            .ok()
            .and_then(|index| u32::try_from(index).ok())
        else {
            return LinkTarget::Unsupported;
        };
        let view = destination.view_settings().ok();
        let top = view.and_then(|view| destination_top(self.upright(page)?, view));
        LinkTarget::Page { page, top }
    }
}

pub(super) fn targets(engine: &PdfiumEngine) -> Targets<impl FnMut(u32) -> Option<Upright> + '_> {
    Targets::new(move |index| engine.page(index).ok().map(|page| upright_of(&page)))
}

fn upright_of(page: &PdfPage<'_>) -> Upright {
    Upright::new(
        page.rotation().map_or(0, quarter_turns),
        page.width().value,
        page.height().value,
    )
}

pub(super) fn extract(engine: &PdfiumEngine, page: &PdfPage<'_>) -> Result<PageLinks, PdfError> {
    let (width, height) = (page.width().value, page.height().value);
    let upright = upright_of(page);
    let mut targets = targets(engine);
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
            target: targets.target(link.action(), || link.destination()),
        });
    }
    PageLinks::new(width, height, links).map_err(|_| PdfError::Corrupt)
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
        View::FitPageToRectangle(rect) => {
            let corners = upright.map(
                (rect.left().value, rect.bottom().value),
                (rect.right().value, rect.top().value),
            );
            return corners.top.is_finite().then_some(corners.top);
        }
        View::SpecificCoordinatesAndZoom(x, y, _) => (x, y),
        View::FitPageHorizontallyToWindow(y) | View::FitBoundsHorizontallyToWindow(y) => (None, y),
        View::FitPageVerticallyToWindow(x) | View::FitBoundsVerticallyToWindow(x) => (x, None),
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
