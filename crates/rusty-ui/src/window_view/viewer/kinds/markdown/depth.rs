use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};

pub(super) const MAX_NESTING: usize = 32;
const PARSE_STACK: usize = 16 * 1024 * 1024;
fn options() -> ParseOptions {
    let mut options = ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    options
}

fn step(node: &Node) -> usize {
    match node {
        Node::Blockquote(_)
        | Node::List(_)
        | Node::Emphasis(_)
        | Node::Strong(_)
        | Node::Delete(_)
        | Node::Link(_)
        | Node::LinkReference(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::Table(_)
        | Node::FootnoteDefinition(_) => 1,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Shape {
    pub depth: usize,
    pub has_raw: bool,
}

impl Shape {
    pub(super) fn is_safe(self) -> bool {
        !self.has_raw && self.depth <= MAX_NESTING
    }
}

fn is_raw(node: &Node) -> bool {
    matches!(node, Node::Html(_) | Node::InlineMath(_) | Node::Math(_))
}

pub(super) fn shape_of(root: &Node) -> Shape {
    let mut shape = Shape::default();
    let mut stack = vec![(root, 0usize)];
    while let Some((node, depth)) = stack.pop() {
        let depth = depth + step(node);
        shape.depth = shape.depth.max(depth);
        shape.has_raw |= is_raw(node);
        stack.extend(
            node.children()
                .into_iter()
                .flatten()
                .map(|child| (child, depth)),
        );
    }
    shape
}

fn parse_shape(text: &str) -> Result<Shape, String> {
    let tree = to_mdast(text, &options()).map_err(|error| error.to_string())?;
    Ok(shape_of(&tree))
}

pub(super) fn measure(text: String) -> (String, Result<Shape, String>) {
    let parser = std::thread::Builder::new()
        .name("markdown-depth".to_owned())
        .stack_size(PARSE_STACK)
        .spawn(move || {
            let shape = parse_shape(&text);
            (text, shape)
        });
    match parser.map(std::thread::JoinHandle::join) {
        Ok(Ok(measured)) => measured,
        Ok(Err(_)) => (
            String::new(),
            Err("The markdown parser stopped.".to_owned()),
        ),
        Err(error) => (
            String::new(),
            Err(format!("Cannot start the markdown parser: {error}")),
        ),
    }
}

#[cfg(test)]
mod tests;
