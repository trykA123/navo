use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, px, relative, size,
};

type Build = Box<dyn FnOnce(Pixels, &mut Window, &mut App) -> AnyElement>;

pub(crate) struct Fit {
    height: Pixels,
    build: Option<Build>,
}

impl Fit {
    pub(crate) fn new(
        height: Pixels,
        build: impl FnOnce(Pixels, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            height,
            build: Some(Box::new(build)),
        }
    }
}

impl IntoElement for Fit {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Fit {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = self.height.into();
        style.min_size.width = px(0.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let build = self.build.take()?;
        let mut row = build(bounds.size.width, window, cx);
        let space = size(
            AvailableSpace::Definite(bounds.size.width),
            AvailableSpace::Definite(bounds.size.height),
        );
        row.prepaint_as_root(bounds.origin, space, window, cx);
        Some(row)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        row: &mut Option<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(row) = row {
            row.paint(window, cx);
        }
    }
}
