use rusty_pdf::protocol::{LinkTarget, OutlineItem};

#[derive(Default)]
pub(super) enum Chapters {
    #[default]
    Unread,
    Read(Vec<OutlineItem>),
}

impl Chapters {
    pub(super) fn is_read(&self) -> bool {
        matches!(self, Self::Read(_))
    }

    pub(super) fn items(&self) -> &[OutlineItem] {
        match self {
            Self::Unread => &[],
            Self::Read(items) => items,
        }
    }

    pub(super) fn current(&self, (page, y): (u32, f32)) -> Option<usize> {
        self.items()
            .iter()
            .enumerate()
            .filter_map(|(row, item)| match item.target {
                LinkTarget::Page { page, top } => Some((row, page, top.unwrap_or(f32::MIN))),
                _ => None,
            })
            .filter(|&(_, at, top)| at < page || (at == page && top <= y))
            .max_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)))
            .map(|(row, ..)| row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(target: LinkTarget) -> OutlineItem {
        OutlineItem {
            title: String::new(),
            level: 0,
            target,
        }
    }

    #[test]
    fn the_current_chapter_is_the_last_one_at_or_before_the_page() {
        let at = |page| item(LinkTarget::Page { page, top: None });
        let chapters = Chapters::Read(vec![
            at(0),
            at(4),
            item(LinkTarget::Unsupported),
            at(9),
            item(LinkTarget::Uri("https://example.org".into())),
        ]);
        assert_eq!(chapters.current((0, 0.)), Some(0));
        assert_eq!(chapters.current((8, 0.)), Some(1));
        assert_eq!(chapters.current((30, 0.)), Some(3));
        assert_eq!(Chapters::Unread.current((3, 0.)), None);
    }

    #[test]
    fn the_current_chapter_follows_page_and_top_not_outline_order() {
        let at = |page, top| item(LinkTarget::Page { page, top });
        let chapters = Chapters::Read(vec![
            at(0, None),
            at(9, None),
            at(4, Some(300.)),
            at(4, Some(100.)),
        ]);
        assert_eq!(chapters.current((4, 50.)), Some(0));
        assert_eq!(chapters.current((4, 200.)), Some(3));
        assert_eq!(chapters.current((5, 0.)), Some(2));
        assert_eq!(chapters.current((9, -10.)), Some(1));
    }
}
