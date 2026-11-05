use crate::{Kind, PatchError, Scope, edit, error, parse};
use pep508_rs::{PackageName, Requirement, VersionOrUrl};
use std::str::FromStr;
use toml_edit::{Array, Document, Value};

pub(crate) fn normalized_name(name: &str) -> Result<String, PatchError> {
    PackageName::from_str(name)
        .map(|s| s.to_string())
        .map_err(|_| error("invalid_name", "invalid package, group, or extra name", 3))
}

fn requirement(text: &str, supplied: bool) -> Result<Requirement, PatchError> {
    let mut r = Requirement::from_str(text).map_err(|_| {
        error(
            "invalid_requirement",
            "invalid PEP 508 requirement",
            if supplied { 2 } else { 3 },
        )
    })?;
    r.extras.sort();
    r.extras.dedup();
    Ok(r)
}

fn marker_present(text: &str, r: &Requirement) -> bool {
    if let Some(VersionOrUrl::Url(url)) = &r.version_or_url {
        !text
            .trim_end()
            .ends_with(url.given().unwrap_or(url.as_str()))
    } else {
        text.contains(';')
    }
}

fn identity(a: &Requirement, at: &str, b: &Requirement, bt: &str) -> bool {
    a.name == b.name
        && a.extras == b.extras
        && a.marker == b.marker
        && marker_present(at, a) == marker_present(bt, b)
}

fn scope_path(doc: &Document<&str>, kind: Kind, scope: &Scope) -> Result<Vec<String>, PatchError> {
    match (kind, scope) {
        (Kind::Script, Scope::Script) => Ok(vec!["dependencies".into()]),
        (Kind::Pyproject, Scope::Project) => Ok(vec!["project".into(), "dependencies".into()]),
        (Kind::Pyproject, Scope::Optional { name }) => Ok(vec![
            "project".into(),
            "optional-dependencies".into(),
            select_name(doc, &["project", "optional-dependencies"], name)?,
        ]),
        (Kind::Pyproject, Scope::Group { name }) => Ok(vec![
            "dependency-groups".into(),
            select_name(doc, &["dependency-groups"], name)?,
        ]),
        _ => Err(error(
            "invalid_scope",
            "dependency scope requires the matching project or script kind",
            2,
        )),
    }
}

fn select_name(doc: &Document<&str>, parent: &[&str], name: &str) -> Result<String, PatchError> {
    let desired = normalized_name(name)
        .map_err(|_| error("invalid_name", "invalid group or extra selector", 2))?;
    let path: Vec<String> = parent.iter().map(|k| (*k).into()).collect();
    if let Some(table) = edit::lookup(doc.as_item(), &path)? {
        let table = table.as_table_like().ok_or_else(|| {
            error(
                "invalid_metadata",
                "dependency container must be a table",
                3,
            )
        })?;
        let mut names = std::collections::HashSet::new();
        let mut found = None;
        for (key, _) in table.iter() {
            let normalized = normalized_name(key)?;
            if !names.insert(normalized.clone()) {
                return Err(error(
                    "ambiguous_name",
                    "group or extra names collide after normalization",
                    4,
                ));
            }
            if normalized == desired {
                found = Some(key.to_owned());
            }
        }
        Ok(found.unwrap_or_else(|| name.to_owned()))
    } else {
        Ok(name.to_owned())
    }
}

fn validate_array(array: &Array, groups: bool) -> Result<(), PatchError> {
    for value in array.iter() {
        if let Some(text) = value.as_str() {
            requirement(text, false)?;
        } else if groups {
            let t = value.as_inline_table().ok_or_else(|| {
                error("unsupported_metadata", "unknown dependency group entry", 5)
            })?;
            if t.len() != 1 || t.get("include-group").and_then(Value::as_str).is_none() {
                return Err(error(
                    "unsupported_metadata",
                    "unknown dependency group include form",
                    5,
                ));
            }
            normalized_name(t["include-group"].as_str().unwrap())?;
        } else {
            return Err(error(
                "invalid_metadata",
                "dependency arrays must contain requirements as strings",
                3,
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_touched(
    doc: &Document<&str>,
    kind: Kind,
    path: &[String],
) -> Result<(), PatchError> {
    if kind == Kind::Script {
        return crate::script::validate(doc);
    }
    if kind != Kind::Pyproject {
        return Ok(());
    }
    for (container, groups) in [
        (vec!["project", "dependencies"], false),
        (vec!["project", "optional-dependencies"], false),
        (vec!["dependency-groups"], true),
    ] {
        let shared = path.len().min(container.len());
        if !path[..shared]
            .iter()
            .map(String::as_str)
            .eq(container[..shared].iter().copied())
        {
            continue;
        }
        let keys: Vec<String> = container.iter().map(|s| (*s).into()).collect();
        if let Some(item) = edit::lookup(doc.as_item(), &keys)? {
            if container.last() == Some(&"dependencies") {
                validate_array(
                    item.as_array().ok_or_else(|| {
                        error("invalid_metadata", "dependencies must be an array", 3)
                    })?,
                    false,
                )?;
            } else {
                let t = item.as_table_like().ok_or_else(|| {
                    error(
                        "invalid_metadata",
                        "dependency container must be a table",
                        3,
                    )
                })?;
                let mut seen = std::collections::HashSet::new();
                for (key, value) in t.iter() {
                    if !seen.insert(normalized_name(key)?) {
                        return Err(error(
                            "ambiguous_name",
                            "group or extra names collide after normalization",
                            4,
                        ));
                    }
                    if path.len() <= container.len() || path[container.len()] == key {
                        validate_array(
                            value.as_array().ok_or_else(|| {
                                error(
                                    "invalid_metadata",
                                    "selected dependency container must be an array",
                                    3,
                                )
                            })?,
                            groups,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn check_dynamic(
    doc: &Document<&str>,
    touched: &[(usize, Vec<String>)],
) -> Result<(), PatchError> {
    if !touched
        .iter()
        .any(|(_, path)| path.first().is_some_and(|p| p == "project"))
    {
        return Ok(());
    }
    let path = vec!["project".into(), "dynamic".into()];
    if let Some(item) = edit::lookup(doc.as_item(), &path)? {
        let array = item.as_array().ok_or_else(|| {
            error(
                "invalid_metadata",
                "project.dynamic must be an array of strings",
                3,
            )
        })?;
        for field in array.iter() {
            let field = field.as_str().ok_or_else(|| {
                error(
                    "invalid_metadata",
                    "project.dynamic must be an array of strings",
                    3,
                )
            })?;
            for (index, path) in touched {
                if path.first().is_some_and(|p| p == "project")
                    && (path.len() == 1 || path[1] == field)
                {
                    let mut e = error(
                        "dynamic_field",
                        "cannot write a field listed in final project.dynamic",
                        4,
                    );
                    e.report.error.as_mut().unwrap().operation = Some(*index);
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn apply(
    source: &mut String,
    kind: Kind,
    scope: &Scope,
    text: &str,
    remove: bool,
) -> Result<Vec<String>, PatchError> {
    let wanted = requirement(text, true)?;
    if remove && wanted.version_or_url.is_some() {
        return Err(error(
            "invalid_selector",
            "removal selector cannot include a version or URL",
            2,
        ));
    }
    let original = source.clone();
    let doc = parse(&original)?;
    let path = scope_path(&doc, kind, scope)?;
    validate_touched(&doc, kind, &path)?;
    let item = edit::lookup(doc.as_item(), &path)?;
    if item.is_none() {
        if remove {
            return Err(error(
                "missing_dependency",
                "selected dependency is absent",
                4,
            ));
        }
        let nl = edit::newline(source);
        edit::set(
            source,
            &path,
            &format!("[{nl}    {},{nl}]", Value::from(text)),
        )?;
        return Ok(path);
    }
    let array = item.unwrap().as_array().ok_or_else(|| {
        error(
            "invalid_metadata",
            "selected dependencies must be an array",
            3,
        )
    })?;
    validate_array(array, matches!(scope, Scope::Group { .. }))?;
    let mut matches = Vec::new();
    for (i, value) in array.iter().enumerate() {
        if let Some(existing) = value.as_str() {
            let parsed = requirement(existing, false)?;
            if identity(&wanted, text, &parsed, existing) {
                matches.push((i, parsed));
            }
        }
    }
    if matches.len() > 1 {
        return Err(error(
            "ambiguous_dependency",
            "multiple direct entries have the selected dependency identity",
            4,
        ));
    }
    if let Some((index, old)) = matches.first() {
        if !remove && *old == wanted {
            return Ok(path);
        }
        let span = array.get(*index).unwrap().span().ok_or_else(edit::layout)?;
        if remove {
            remove_entry(source, array, *index)?;
        } else {
            let rendered =
                if original[span.clone()].starts_with('\'') && !text.contains(['\'', '\n', '\r']) {
                    format!("'{text}'")
                } else {
                    Value::from(text).to_string()
                };
            edit::splice(source, span, &rendered)?;
        }
    } else if remove {
        return Err(error(
            "missing_dependency",
            "selected dependency identity is absent",
            4,
        ));
    } else {
        append(source, array, text)?;
    }
    Ok(path)
}

fn append(source: &mut String, array: &Array, text: &str) -> Result<(), PatchError> {
    let span = array.span().ok_or_else(edit::layout)?;
    let close = span.end - 1;
    let literal_style = array
        .iter()
        .find(|v| v.is_str())
        .and_then(Value::span)
        .is_some_and(|s| source[s].starts_with('\''));
    let rendered = if literal_style && !text.contains(['\'', '\r', '\n']) {
        format!("'{text}'")
    } else {
        Value::from(text).to_string()
    };
    if !source[span.clone()].contains('\n') {
        let at = array
            .iter()
            .last()
            .and_then(Value::span)
            .map_or(span.start + 1, |s| s.end);
        let tail = &source[at..close];
        if tail.contains('#') {
            return Err(edit::layout());
        }
        if tail.contains(',') {
            return edit::splice(source, close..close, &format!(" {rendered},"));
        }
        let insertion = if array.is_empty() {
            rendered
        } else {
            format!(", {rendered}")
        };
        return edit::splice(source, at..at, &insertion);
    }
    let line = edit::line_start(source, close);
    if !source[line..close].trim().is_empty() {
        return Err(edit::layout());
    }
    let indent = array
        .iter()
        .next()
        .and_then(Value::span)
        .map(|s| &source[edit::line_start(source, s.start)..s.start])
        .filter(|s| s.trim().is_empty())
        .unwrap_or("    ")
        .to_string();
    let mut candidate = source.clone();
    candidate.insert_str(
        line,
        &format!("{indent}{rendered},{}", edit::newline(source)),
    );
    if let Some(last) = array.iter().last().and_then(Value::span) {
        let tail = &source[last.end..close];
        if !tail.trim_start().starts_with(',') {
            candidate.insert(last.end, ',');
        }
    }
    edit::splice(
        source,
        span.clone(),
        &candidate[span.start..span.end + candidate.len() - source.len()],
    )
}

fn remove_entry(source: &mut String, array: &Array, index: usize) -> Result<(), PatchError> {
    let span = array.get(index).unwrap().span().ok_or_else(edit::layout)?;
    let start = edit::line_start(source, span.start);
    let end = edit::line_end(source, span.end);
    let suffix = source[span.end..end].trim();
    if source[start..span.start].trim().is_empty()
        && (suffix.is_empty()
            || suffix == ","
            || suffix.starts_with('#')
            || suffix.starts_with(", #")
            || suffix.starts_with(",#"))
    {
        return edit::splice(source, start..end, "");
    }
    let envelope = array.span().ok_or_else(edit::layout)?;
    let (range, gap) = if let Some(next) = array.get(index + 1).and_then(Value::span) {
        (
            span.start..next.start,
            source[span.end..next.start].to_string(),
        )
    } else if index > 0 {
        let previous = array
            .get(index - 1)
            .unwrap()
            .span()
            .ok_or_else(edit::layout)?;
        (
            previous.end..span.end,
            source[previous.end..span.start].to_string(),
        )
    } else {
        (
            span.start..envelope.end - 1,
            source[span.end..envelope.end - 1].to_string(),
        )
    };
    if gap.contains('#')
        || !gap.chars().all(|c| c.is_whitespace() || c == ',')
        || range.start <= envelope.start
    {
        return Err(edit::layout());
    }
    edit::splice(source, range, "")
}
