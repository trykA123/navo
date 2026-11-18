//! Imports Notepad++ UDL XML into editor syntax rules.

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

/// Reads and unescapes a named attribute; a malformed attribute or a bad entity in any value
/// on the tag fails, even one after the wanted key or on an attribute nobody reads.
fn attribute(tag: &BytesStart<'_>, key: &str) -> Result<Option<String>, RulesError> {
    let mut found = None;
    for attr in tag.attributes() {
        let attr = attr.map_err(xml_error)?;
        let value = unescape(&attr.value).map_err(xml_error)?;
        if found.is_none() && attr.key.as_ref() == key {
            found = Some(value.into_owned());
        }
    }
    Ok(found)
}

/// Checks the attributes of every element inside a `Keywords` element, which `read_text` skips
/// over: a malformed attribute or a bad entity on any of them fails, like on any other tag.
fn check_nested(inner: &str) -> Result<(), RulesError> {
    let mut reader = Reader::from_str(inner);
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(tag) | Event::Empty(tag) => {
                attribute(&tag, "")?;
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

/// Parses UDL text into languages; malformed XML or incomplete rules fail.
pub fn from_udl(xml: &str) -> Result<Vec<Language>, RulesError> {
    let mut reader = Reader::from_str(xml);
    let mut languages = Vec::new();
    let mut draft: Option<Draft> = None;
    let mut depth = 0_usize;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(tag) if tag.name().as_ref() == "Keywords" => {
                let name = attribute(&tag, "name")?.unwrap_or_default();
                let raw = reader.read_text(tag.name()).map_err(xml_error)?;
                check_nested(&raw)?;
                let text = unescape(&raw).map_err(xml_error)?;
                if let Some(draft) = draft.as_mut() {
                    draft.keywords(&name, &text);
                }
            }
            Event::Start(tag) => {
                depth += 1;
                open(&tag, &mut draft)?;
            }
            Event::Empty(tag) => {
                open(&tag, &mut draft)?;
                close(tag.name().as_ref(), &mut draft, &mut languages)?;
            }
            Event::End(tag) => {
                depth = depth.saturating_sub(1);
                close(tag.name().as_ref(), &mut draft, &mut languages)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth > 0 || draft.is_some() {
        return Err(xml_error("the file ends before every element is closed"));
    }
    if languages.is_empty() {
        return Err(xml_error("no <UserLang> element"));
    }
    Ok(languages)
}

/// Finishes a closing language element; invalid language rules fail.
fn close(
    name: &str,
    draft: &mut Option<Draft>,
    languages: &mut Vec<Language>,
) -> Result<(), RulesError> {
    if name == "UserLang"
        && let Some(done) = draft.take()
    {
        languages.push(done.finish()?);
    }
    Ok(())
}

/// Reads rule attributes from an opening tag; a malformed attribute or a bad entity on any tag
/// fails, inside a language or outside it.
fn open(tag: &BytesStart<'_>, draft: &mut Option<Draft>) -> Result<(), RulesError> {
    attribute(tag, "")?;
    match tag.name().as_ref() {
        "UserLang" => {
            let extensions = attribute(tag, "ext")?.unwrap_or_default();
            *draft = Some(Draft::new(attribute(tag, "name")?, &extensions));
        }
        "Global" => {
            if let Some(draft) = draft.as_mut() {
                draft.case_ignored = attribute(tag, "caseIgnored")?.as_deref() == Some("yes");
            }
        }
        "Prefix" => {
            if let Some(draft) = draft.as_mut() {
                for (index, flag) in draft.prefix.iter_mut().enumerate() {
                    let key = format!("Keywords{}", index + 1);
                    *flag = attribute(tag, &key)?.as_deref() == Some("yes");
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
