mod codes;

use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};

use crate::{Language, RulesError};
use codes::Draft;

fn xml_error(error: impl std::fmt::Display) -> RulesError {
    RulesError::Xml {
        message: error.to_string(),
    }
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>, RulesError> {
    for attr in tag.attributes() {
        let attr = attr.map_err(xml_error)?;
        if attr.key.as_ref() == key {
            let raw = std::str::from_utf8(&attr.value).map_err(xml_error)?;
            return Ok(Some(unescape(raw).map_err(xml_error)?.into_owned()));
        }
    }
    Ok(None)
}

pub fn from_udl(xml: &str) -> Result<Vec<Language>, RulesError> {
    let mut reader = Reader::from_str(xml);
    let mut languages = Vec::new();
    let mut draft: Option<Draft> = None;
    let mut depth = 0_usize;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(tag) if tag.name().as_ref() == b"Keywords" => {
                let name = attribute(&tag, b"name")?.unwrap_or_default();
                let raw = reader.read_text(tag.name()).map_err(xml_error)?;
                let raw = raw.decode().map_err(xml_error)?;
                let text = unescape(&raw).map_err(xml_error)?;
                if let Some(draft) = draft.as_mut() {
                    draft.keywords(&name, &text);
                }
            }
            Event::Start(tag) => {
                depth += 1;
                open(&tag, &mut draft)?;
            }
            Event::Empty(tag) => open(&tag, &mut draft)?,
            Event::End(tag) => {
                depth = depth.saturating_sub(1);
                if tag.name().as_ref() == b"UserLang"
                    && let Some(done) = draft.take()
                {
                    languages.push(done.finish()?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth > 0 || draft.is_some() {
        return Err(xml_error("the file ends before every element is closed"));
    }
    Ok(languages)
}

fn open(tag: &BytesStart<'_>, draft: &mut Option<Draft>) -> Result<(), RulesError> {
    match tag.name().as_ref() {
        b"UserLang" => {
            let extensions = attribute(tag, b"ext")?.unwrap_or_default();
            *draft = Some(Draft::new(attribute(tag, b"name")?, &extensions));
        }
        b"Global" => {
            if let Some(draft) = draft.as_mut() {
                draft.case_ignored = attribute(tag, b"caseIgnored")?.as_deref() == Some("yes");
            }
        }
        b"Prefix" => {
            if let Some(draft) = draft.as_mut() {
                for (index, flag) in draft.prefix.iter_mut().enumerate() {
                    let key = format!("Keywords{}", index + 1);
                    *flag = attribute(tag, key.as_bytes())?.as_deref() == Some("yes");
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
