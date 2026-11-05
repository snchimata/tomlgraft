use crate::{PatchError, error, parse};
use std::ops::Range;
use toml_edit::{Document, Item, Key, Table, Value};

pub fn parse_path(text: &str) -> Result<Vec<String>, PatchError> {
    Key::parse(text)
        .map(|keys| keys.iter().map(|k| k.get().to_owned()).collect())
        .map_err(|_| error("invalid_path", "path must use TOML dotted-key syntax", 2))
}

fn path_check(path: &[String]) -> Result<(), PatchError> {
    if path.is_empty() {
        Err(error(
            "invalid_path",
            "path must contain at least one key",
            2,
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn lookup<'a>(root: &'a Item, path: &[String]) -> Result<Option<&'a Item>, PatchError> {
    let mut current = root;
    for key in path {
        let table = current
            .as_table_like()
            .ok_or_else(|| error("type_conflict", "path traverses a scalar or array", 4))?;
        match table.get(key) {
            Some(item) => current = item,
            None => return Ok(None),
        }
    }
    Ok(Some(current))
}

pub(crate) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(a), Value::String(b)) => a.value() == b.value(),
        (Value::Integer(a), Value::Integer(b)) => a.value() == b.value(),
        (Value::Float(a), Value::Float(b)) => {
            a.value() == b.value() || (a.value().is_nan() && b.value().is_nan())
        }
        (Value::Boolean(a), Value::Boolean(b)) => a.value() == b.value(),
        (Value::Datetime(a), Value::Datetime(b)) => a.value() == b.value(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| same(a, b))
        }
        (Value::InlineTable(a), Value::InlineTable(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|other| same(v, other)))
        }
        _ => false,
    }
}

pub(crate) fn splice(
    source: &mut String,
    span: Range<usize>,
    replacement: &str,
) -> Result<(), PatchError> {
    if source.get(span.clone()).is_none() {
        return Err(layout());
    }
    let mut candidate = source.clone();
    candidate.replace_range(span.clone(), replacement);
    parse(&candidate).map_err(|_| layout())?;
    if source[..span.start] != candidate[..span.start]
        || source[span.end..] != candidate[span.start + replacement.len()..]
    {
        return Err(layout());
    }
    *source = candidate;
    Ok(())
}

pub(crate) fn layout() -> PatchError {
    error(
        "unsupported_layout",
        "cannot prove a local edit for this layout",
        5,
    )
}
pub(crate) fn newline(source: &str) -> &'static str {
    if source
        .find('\n')
        .is_some_and(|n| n > 0 && source.as_bytes()[n - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    }
}
pub(crate) fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map_or(0, |p| p + 1)
}
pub(crate) fn line_end(source: &str, offset: usize) -> usize {
    source[offset..]
        .find('\n')
        .map_or(source.len(), |p| offset + p + 1)
}

pub(crate) fn set(source: &mut String, path: &[String], text: &str) -> Result<(), PatchError> {
    path_check(path)?;
    let value = text.parse::<Value>().map_err(|_| {
        error(
            "invalid_value",
            "value_toml must contain exactly one TOML value",
            2,
        )
    })?;
    let original = source.clone();
    let doc = parse(&original)?;
    if let Some(existing) = lookup(doc.as_item(), path)? {
        let existing = existing.as_value().ok_or_else(|| {
            error(
                "unsupported_structure",
                "standard tables and arrays of tables cannot be replaced",
                5,
            )
        })?;
        if same(existing, &value) {
            return Ok(());
        }
        return splice(
            source,
            existing.span().ok_or_else(layout)?,
            &value.to_string(),
        );
    }
    // Inline parents have no document-level insertion boundary.
    let n = path.len() - 1;
    if let Some(table) = lookup(doc.as_item(), &path[..n])?.and_then(Item::as_inline_table) {
        let span = table.span().ok_or_else(layout)?;
        let mut document = parse(source)?.into_mut();
        let mut parent = document.as_item_mut();
        for key in &path[..n] {
            parent = parent.as_table_like_mut().unwrap().get_mut(key).unwrap();
        }
        parent
            .as_inline_table_mut()
            .unwrap()
            .insert(&path[n], value);
        return splice(source, span, parent.to_string().trim());
    }
    for n in 1..path.len() {
        if lookup(doc.as_item(), &path[..n])?.is_some_and(Item::is_inline_table) {
            return Err(layout());
        }
    }
    insert(source, &doc, path, &value)
}

fn headers(table: &Table, starts: &mut Vec<usize>) {
    for (_, item) in table.iter() {
        if let Some(t) = item.as_table() {
            if !t.is_implicit()
                && !t.is_dotted()
                && let Some(s) = t.span()
            {
                starts.push(s.start);
            }
            headers(t, starts);
        } else if let Some(a) = item.as_array_of_tables() {
            for t in a.iter() {
                if let Some(s) = t.span() {
                    starts.push(s.start);
                }
                headers(t, starts);
            }
        }
    }
}

fn insert(
    source: &mut String,
    doc: &Document<&str>,
    path: &[String],
    value: &Value,
) -> Result<(), PatchError> {
    let mut parent_len = 0;
    let mut header_start = None;
    for n in 1..path.len() {
        if let Some(item) = lookup(doc.as_item(), &path[..n])? {
            let table = item
                .as_table()
                .ok_or_else(|| error("type_conflict", "missing key has a non-table parent", 4))?;
            if !table.is_implicit() && !table.is_dotted() {
                parent_len = n;
                header_start = Some(table.span().ok_or_else(layout)?.start);
            }
        } else {
            break;
        }
    }
    let mut starts = Vec::new();
    headers(doc.as_table(), &mut starts);
    let at = starts
        .into_iter()
        .filter(|s| header_start.is_none_or(|h| *s > h))
        .min()
        .map_or(source.len(), |s| line_start(source, s));
    let nl = newline(source);
    let key = path[parent_len..]
        .iter()
        .map(|k| Key::new(k).to_string())
        .collect::<Vec<_>>()
        .join(".");
    let before = if at > 0 && !source[..at].ends_with('\n') {
        nl
    } else {
        ""
    };
    let after = if at < source.len() || source.ends_with('\n') {
        nl
    } else {
        ""
    };
    splice(source, at..at, &format!("{before}{key} = {value}{after}"))
}

pub(crate) fn remove(source: &mut String, path: &[String]) -> Result<(), PatchError> {
    path_check(path)?;
    let original = source.clone();
    let doc = parse(&original)?;
    let item = lookup(doc.as_item(), path)?
        .ok_or_else(|| error("missing_path", "removal requires an existing leaf", 4))?;
    let value = item.as_value().ok_or_else(|| {
        error(
            "unsupported_structure",
            "standard tables and arrays of tables cannot be removed",
            5,
        )
    })?;
    let n = path.len() - 1;
    if let Some(parent) = lookup(doc.as_item(), &path[..n])?.and_then(Item::as_inline_table) {
        let span = parent.span().ok_or_else(layout)?;
        let mut document = parse(source)?.into_mut();
        let mut item = document.as_item_mut();
        for key in &path[..path.len() - 1] {
            item = item.as_table_like_mut().unwrap().get_mut(key).unwrap();
        }
        item.as_table_like_mut()
            .unwrap()
            .remove(path.last().unwrap());
        let mut parent_item = document.as_item();
        for key in &path[..n] {
            parent_item = parent_item.as_table_like().unwrap().get(key).unwrap();
        }
        return splice(source, span, parent_item.to_string().trim());
    }
    let span = value.span().ok_or_else(layout)?;
    let parent = lookup(doc.as_item(), &path[..path.len() - 1])?
        .unwrap()
        .as_table_like()
        .unwrap();
    let key_span = parent
        .key(path.last().unwrap())
        .and_then(Key::span)
        .ok_or_else(layout)?;
    let start = line_start(source, key_span.start);
    // A multiline dotted key would make this envelope ambiguous.
    if source[start..key_span.start].contains(['[', '#', '=']) {
        return Err(layout());
    }
    let end = line_end(source, span.end);
    let start = if end == source.len() && !source.ends_with('\n') && start > 0 {
        if source[..start].ends_with("\r\n") {
            start - 2
        } else {
            start - 1
        }
    } else {
        start
    };
    let replacement = if start == 0 && end == source.len() && source.ends_with('\n') {
        newline(source)
    } else {
        ""
    };
    splice(source, start..end, replacement)
}
