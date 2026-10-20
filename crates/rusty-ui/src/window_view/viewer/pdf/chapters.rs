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

    pub(super) fn current(&self, page: u32) -> Option<usize> {
        self.items().iter().rposition(
            |item| matches!(item.target, LinkTarget::Page { page: at, .. } if at <= page),
        )
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
        assert_eq!(chapters.current(0), Some(0));
        assert_eq!(chapters.current(8), Some(1));
        assert_eq!(chapters.current(30), Some(3));
        assert_eq!(Chapters::Unread.current(3), None);
    }
}
