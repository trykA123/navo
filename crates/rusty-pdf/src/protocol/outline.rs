use super::{CodecError, LinkTarget, MAX_OUTLINE_ITEMS, MAX_OUTLINE_LEVEL, MAX_TITLE_CHARS};

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    pub level: u8,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    items: Vec<OutlineItem>,
}

impl Outline {
    pub fn new(items: Vec<OutlineItem>) -> Result<Self, CodecError> {
        if items.len() > MAX_OUTLINE_ITEMS {
            return Err(CodecError::TooLarge {
                what: "outline",
                len: items.len() as u64,
                max: MAX_OUTLINE_ITEMS as u64,
            });
        }
        for item in &items {
            let count = item.title.chars().count();
            if count > MAX_TITLE_CHARS {
                return Err(CodecError::TooLarge {
                    what: "title",
                    len: count as u64,
                    max: MAX_TITLE_CHARS as u64,
                });
            }
            if item.level > MAX_OUTLINE_LEVEL {
                return Err(CodecError::Malformed("outline level is over its limit"));
            }
            item.target.validate()?;
        }
        Ok(Self { items })
    }

    pub fn items(&self) -> &[OutlineItem] {
        &self.items
    }
}
