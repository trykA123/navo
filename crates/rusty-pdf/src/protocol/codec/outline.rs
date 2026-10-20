use super::links::{put_target, target};
use super::{Input, put_str, put_u32};
use crate::protocol::{CodecError, MAX_OUTLINE_ITEMS, MAX_TITLE_CHARS, Outline, OutlineItem};

pub(super) fn put_outline(out: &mut Vec<u8>, outline: &Outline) -> Result<(), CodecError> {
    out.push(8);
    put_u32(out, outline.items().len() as u32);
    for item in outline.items() {
        put_str(out, "title", &item.title, MAX_TITLE_CHARS * 4)?;
        out.push(item.level);
        put_target(out, &item.target)?;
    }
    Ok(())
}

pub(super) fn outline(input: &mut Input<'_>) -> Result<Outline, CodecError> {
    let count = input.u32()? as usize;
    if count > MAX_OUTLINE_ITEMS {
        return Err(CodecError::TooLarge {
            what: "outline",
            len: count as u64,
            max: MAX_OUTLINE_ITEMS as u64,
        });
    }
    let items = (0..count)
        .map(|_| outline_item(input))
        .collect::<Result<Vec<_>, _>>()?;
    Outline::new(items)
}

fn outline_item(input: &mut Input<'_>) -> Result<OutlineItem, CodecError> {
    Ok(OutlineItem {
        title: input.string("title", MAX_TITLE_CHARS * 4)?,
        level: input.u8()?,
        target: target(input)?,
    })
}
