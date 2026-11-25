//! The expansion pre-pass (BB-D9 step 1): before usvg copies anything, weighs what `<use>`,
//! markers, masks, clip paths, filters and paint servers would copy and measures nesting,
//! without recursion.

use std::collections::HashMap;

use resvg::usvg::roxmltree::{Document, Node};

use super::SvgError;

/// The most an SVG may weigh once expanded: one per element copy plus one per number in its
/// path data. About five times what fits in `SVG_CAP` of output, far above any real icon.
pub const MAX_WEIGHT: u64 = 250_000;
/// The deepest chain of nested elements and references; usvg walks each level recursively.
pub const MAX_DEPTH: u64 = 64;

/// What one element costs once its children and references are expanded.
#[derive(Clone, Copy)]
struct Cost {
    weight: u64,
    elements: u64,
    depth: u64,
}

#[derive(Clone, Copy)]
enum State {
    New,
    Open,
    Done(Cost),
}

/// How often a link copies its target: once, or once per vertex for a marker on a shape.
#[derive(Clone, Copy)]
enum Copies {
    Once,
    PerVertex,
}

/// Each id mapped to its last holder, as usvg resolves links.
type Ids<'a, 'input> = HashMap<&'a str, Node<'a, 'input>>;

/// Refuses `document` when it expands past `MAX_WEIGHT` or nests past `MAX_DEPTH`; a reference
/// cycle is refused as too complex.
pub(super) fn check(document: &Document<'_>) -> Result<(), SvgError> {
    let mut ids: Ids<'_, '_> = HashMap::new();
    for node in document.descendants().filter(Node::is_element) {
        if let Some(id) = node.attribute("id") {
            ids.insert(id, node);
        }
    }
    let mut states = vec![State::New; document.descendants().count()];
    let root = document.root_element();
    let mut stack = vec![(root, false)];
    while let Some((node, entered)) = stack.pop() {
        if entered {
            let cost = expand(node, &ids, &states)?;
            set(&mut states, node, State::Done(cost));
            continue;
        }
        match state(&states, node) {
            State::Done(_) => continue,
            State::Open => return Err(SvgError::TooComplex),
            State::New => {}
        }
        set(&mut states, node, State::Open);
        stack.push((node, true));
        for (link, _) in links(node, &ids) {
            match state(&states, link) {
                State::Open => return Err(SvgError::TooComplex),
                State::New => stack.push((link, false)),
                State::Done(_) => {}
            }
        }
    }
    let root = done(&states, root)?;
    shared(document, &ids, &states, root)
}

/// Adds the resources a style sheet or an inherited marker or paint may give every element: each
/// one may nest in the next, so their cost compounds once per resource. Gradient stops hold no
/// elements, so a shape takes at most a fill and a stroke copy of the heaviest gradient.
fn shared(
    document: &Document<'_>,
    ids: &Ids<'_, '_>,
    states: &[State],
    root: Cost,
) -> Result<(), SvgError> {
    let mut resources = shared_resources(document, ids);
    resources.sort_by_key(|node| node.id().get());
    resources.dedup_by_key(|node| node.id());
    let (mut base, mut factor, mut depth, mut gradient) = (0_u64, 0_u64, root.depth, 0_u64);
    for resource in &resources {
        let cost = done(states, *resource)?;
        if is_gradient(*resource) {
            gradient = gradient.max(cost.weight);
            continue;
        }
        base = base.saturating_add(cost.weight);
        factor = factor.saturating_add(per_copy(cost));
        depth = depth.saturating_add(cost.depth);
    }
    if depth > MAX_DEPTH {
        return Err(SvgError::TooDeep);
    }
    base = base.saturating_add(gradient.saturating_mul(2));
    let mut each = 0_u64;
    for _ in &resources {
        let next = base.saturating_add(factor.saturating_mul(each));
        let settled = next == each || next > MAX_WEIGHT;
        each = next;
        if settled {
            break;
        }
    }
    let total = root
        .weight
        .saturating_add(per_copy(root).saturating_mul(each));
    if total > MAX_WEIGHT {
        return Err(SvgError::TooComplex);
    }
    Ok(())
}

/// Resources named by a style sheet, and markers and paint servers named by an element that is
/// not a shape (its shapes inherit them); a style sheet may apply them to any element.
fn shared_resources<'a, 'input>(
    document: &'a Document<'input>,
    ids: &Ids<'a, 'input>,
) -> Vec<Node<'a, 'input>> {
    let mut found = Vec::new();
    for node in document.descendants().filter(Node::is_element) {
        let sheet = node.tag_name().name() == "style";
        let texts = node
            .children()
            .filter(|child| sheet && child.is_text())
            .filter_map(|child| child.text());
        let values = node
            .attributes()
            .map(|attribute| attribute.value())
            .chain(texts);
        let targets = values.flat_map(|value| targets(value, ids));
        found.extend(targets.filter(|target| {
            (sheet && is_resource(*target)) || (is_inherited(*target) && !is_shape(node))
        }));
    }
    found
}

/// An upper bound on copies of a shared resource inside `cost`: one per element plus one per
/// vertex and the two end markers.
fn per_copy(cost: Cost) -> u64 {
    cost.weight.saturating_add(cost.elements.saturating_mul(2))
}

/// The cost of `node` from its own path data and its links, which are all done.
fn expand(node: Node<'_, '_>, ids: &Ids<'_, '_>, states: &[State]) -> Result<Cost, SvgError> {
    let numbers = numbers(node);
    let mut cost = Cost {
        weight: numbers.saturating_add(1),
        elements: 1,
        depth: 1,
    };
    for (link, copies) in links(node, ids) {
        let target = done(states, link)?;
        let times = match copies {
            Copies::Once => 1,
            Copies::PerVertex => numbers.saturating_add(2),
        };
        cost.weight = cost
            .weight
            .saturating_add(target.weight.saturating_mul(times));
        cost.elements = cost
            .elements
            .saturating_add(target.elements.saturating_mul(times));
        cost.depth = cost.depth.max(target.depth.saturating_add(1));
    }
    if cost.depth > MAX_DEPTH {
        return Err(SvgError::TooDeep);
    }
    if cost.weight > MAX_WEIGHT {
        return Err(SvgError::TooComplex);
    }
    Ok(cost)
}

/// The element children of `node`, its `href` target and the masks, clip paths, filters and
/// (on a shape) markers and paint servers its attributes name, each once per attribute.
fn links<'a, 'input>(
    node: Node<'a, 'input>,
    ids: &Ids<'a, 'input>,
) -> Vec<(Node<'a, 'input>, Copies)> {
    let mut found: Vec<_> = node
        .children()
        .filter(Node::is_element)
        .map(|child| (child, Copies::Once))
        .collect();
    for attribute in node.attributes() {
        if attribute.name() == "href" && follows_href(node) {
            let id = attribute
                .value()
                .trim()
                .strip_prefix('#')
                .unwrap_or_default();
            found.extend(ids.get(id).map(|link| (*link, Copies::Once)));
            continue;
        }
        for link in targets(attribute.value(), ids) {
            match (link.tag_name().name(), is_shape(node)) {
                ("marker", true) => found.push((link, Copies::PerVertex)),
                ("mask" | "clipPath" | "filter", _) => found.push((link, Copies::Once)),
                (_, true) if is_paint(link) => found.push((link, Copies::Once)),
                _ => {}
            }
        }
    }
    found
}

/// Elements whose `href` makes usvg copy or read the target.
fn follows_href(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "use"
            | "feImage"
            | "textPath"
            | "tref"
            | "pattern"
            | "filter"
            | "linearGradient"
            | "radialGradient"
    )
}

/// Resources usvg may convert or copy again for every element that names them.
fn is_resource(node: Node<'_, '_>) -> bool {
    is_inherited(node) || matches!(node.tag_name().name(), "mask" | "clipPath" | "filter")
}

/// Resources a group passes on to every shape inside it: markers and paint servers.
fn is_inherited(node: Node<'_, '_>) -> bool {
    node.tag_name().name() == "marker" || is_paint(node)
}

/// Paint servers, which usvg copies (pattern content, gradient stops) for every shape they paint.
fn is_paint(node: Node<'_, '_>) -> bool {
    node.tag_name().name() == "pattern" || is_gradient(node)
}

fn is_gradient(node: Node<'_, '_>) -> bool {
    matches!(node.tag_name().name(), "linearGradient" | "radialGradient")
}

fn is_shape(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon"
    )
}

/// An upper bound on the segments a shape draws: digit runs in its data plus close commands.
fn numbers(node: Node<'_, '_>) -> u64 {
    let runs = |data: &str| {
        let bytes = data.as_bytes();
        let starts = bytes
            .windows(2)
            .filter(|pair| !pair[0].is_ascii_digit() && pair[1].is_ascii_digit())
            .count();
        let first = usize::from(bytes.first().is_some_and(u8::is_ascii_digit));
        let closes = bytes
            .iter()
            .filter(|byte| matches!(byte, b'z' | b'Z'))
            .count();
        u64::try_from(starts + first + closes).unwrap_or(u64::MAX)
    };
    match node.tag_name().name() {
        "path" => node.attribute("d").map_or(0, runs),
        "polyline" | "polygon" => node.attribute("points").map_or(0, runs),
        "rect" => 9,
        "circle" | "ellipse" => 5,
        "line" => 2,
        _ => 0,
    }
}

/// The last holders of the ids named by `url(#id)` in `value`, each once.
fn targets<'a, 'input>(value: &str, ids: &Ids<'a, 'input>) -> Vec<Node<'a, 'input>> {
    let mut found: Vec<_> = urls(value).filter_map(|id| ids.get(id).copied()).collect();
    found.sort_by_key(|node| node.id().get());
    found.dedup_by_key(|node| node.id());
    found
}

/// The ids named by `url(#id)` in `value`.
fn urls(value: &str) -> impl Iterator<Item = &str> {
    value.split("url(").skip(1).filter_map(|rest| {
        let rest = rest
            .trim_start()
            .trim_start_matches(['"', '\''])
            .trim_start();
        let id = rest.strip_prefix('#')?;
        id.split(|ch: char| matches!(ch, ')' | '"' | '\'') || ch.is_whitespace())
            .next()
    })
}

fn state(states: &[State], node: Node<'_, '_>) -> State {
    states
        .get(node.id().get_usize())
        .copied()
        .unwrap_or(State::Open)
}

fn set(states: &mut [State], node: Node<'_, '_>, value: State) {
    if let Some(slot) = states.get_mut(node.id().get_usize()) {
        *slot = value;
    }
}

/// The finished cost of `node`; a link that is not done is a cycle.
fn done(states: &[State], node: Node<'_, '_>) -> Result<Cost, SvgError> {
    match state(states, node) {
        State::Done(cost) => Ok(cost),
        State::New | State::Open => Err(SvgError::TooComplex),
    }
}
