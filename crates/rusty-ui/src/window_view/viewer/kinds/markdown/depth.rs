use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};

pub(super) const MAX_NESTING: usize = 32;
const PARSE_STACK: usize = 16 * 1024 * 1024;
const VOID_TAGS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

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
struct Tags {
    deepest: usize,
    net: isize,
}

fn tag_at(html: &str) -> Option<(bool, &str, &str)> {
    let rest = html.strip_prefix('<')?;
    let (closing, rest) = match rest.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let name_len = rest.bytes().take_while(u8::is_ascii_alphanumeric).count();
    let name = rest.get(..name_len).filter(|name| !name.is_empty())?;
    let tail = rest.get(name_len..)?;
    Some((closing, name, tail.split('>').next().unwrap_or_default()))
}

fn tags_of(html: &str) -> Tags {
    let mut tags = Tags::default();
    let mut depth: isize = 0;
    for (at, _) in html.match_indices('<') {
        let Some((closing, name, attrs)) = html.get(at..).and_then(tag_at) else {
            continue;
        };
        if closing {
            depth -= 1;
        } else if !attrs.ends_with('/')
            && !VOID_TAGS.iter().any(|tag| tag.eq_ignore_ascii_case(name))
        {
            depth += 1;
            tags.deepest = tags.deepest.max(usize::try_from(depth).unwrap_or(0));
        }
    }
    tags.net = depth;
    tags
}

pub(super) fn deepest_in(root: &Node) -> usize {
    let mut deepest = 0;
    let mut stack = vec![(root, 0usize)];
    while let Some((node, depth)) = stack.pop() {
        let depth = depth + step(node);
        deepest = deepest.max(depth);
        if let Node::Html(html) = node {
            deepest = deepest.max(depth + tags_of(&html.value).deepest);
        }
        let mut open: isize = 0;
        for child in node.children().into_iter().flatten() {
            stack.push((child, depth + usize::try_from(open).unwrap_or(0)));
            if let Node::Html(html) = child {
                open = (open + tags_of(&html.value).net).max(0);
            }
        }
    }
    deepest
}

fn parse_depth(text: &str) -> Result<usize, String> {
    let tree = to_mdast(text, &options()).map_err(|error| error.to_string())?;
    Ok(deepest_in(&tree))
}

pub(super) fn measure(text: String) -> (String, Result<usize, String>) {
    let parser = std::thread::Builder::new()
        .name("markdown-depth".to_owned())
        .stack_size(PARSE_STACK)
        .spawn(move || {
            let depth = parse_depth(&text);
            (text, depth)
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
