use crate::{PatchError, edit, error, parse};
use pep508_rs::{Requirement, pep440_rs::VersionSpecifiers};
use std::{ops::Range, str::FromStr};
use toml_edit::Document;

struct Line<'a> {
    raw: &'a str,
    text: &'a str,
    start: usize,
}
fn lines(source: &str) -> Vec<Line<'_>> {
    let mut offset = 0;
    source
        .split_inclusive('\n')
        .map(|raw| {
            let start = offset;
            offset += raw.len();
            Line {
                raw,
                text: raw.trim_end_matches(['\r', '\n']),
                start,
            }
        })
        .collect()
}
fn content(line: &str) -> Option<&str> {
    if line == "#" {
        Some("")
    } else {
        line.strip_prefix("# ")
    }
}
fn opener(line: &str) -> Option<&str> {
    line.strip_prefix("# /// ")
        .filter(|t| !t.is_empty() && t.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
}

pub(crate) struct Block {
    pub toml: String,
    span: Range<usize>,
    raw_lines: Vec<String>,
    created: bool,
    newline: &'static str,
}

fn cookie(line: &str) -> Option<&str> {
    let comment = line
        .trim_start_matches([' ', '\t', '\u{c}'])
        .strip_prefix('#')?;
    let mut rest = comment;
    while let Some(i) = rest.find("coding") {
        rest = &rest[i + 6..];
        if let Some(tail) = rest.strip_prefix([':', '=']) {
            let name = tail.trim_start_matches([' ', '\t']);
            return Some(
                name.split(|c: char| !c.is_ascii_alphanumeric() && !"-_.".contains(c))
                    .next()
                    .unwrap_or(""),
            );
        }
    }
    None
}

pub(crate) fn extract(source: &str, create: bool) -> Result<Block, PatchError> {
    let bom = if source.starts_with('\u{feff}') { 3 } else { 0 };
    let body = &source[bom..];
    let ls = lines(body);
    let first_comment = ls
        .first()
        .is_none_or(|l| l.text.trim().is_empty() || l.text.trim_start().starts_with('#'));
    let mut cookie_line = None;
    for (i, line) in ls
        .iter()
        .take(if first_comment { 2 } else { 1 })
        .enumerate()
    {
        if let Some(encoding) = cookie(line.text) {
            if !matches!(
                encoding.to_ascii_lowercase().as_str(),
                "utf-8" | "utf8" | "utf_8"
            ) {
                return Err(error(
                    "unsupported_encoding",
                    "script declares a non-UTF-8 encoding",
                    5,
                ));
            }
            cookie_line = Some(i);
        }
    }
    let mut selected = None;
    let mut unclosed_script = false;
    let mut i = 0;
    while i < ls.len() {
        if let Some(kind) = opener(ls[i].text) {
            let mut end = i + 1;
            while end < ls.len() && content(ls[end].text).is_some() {
                end += 1;
            }
            let closing = (i + 1..end).rev().find(|&n| ls[n].text == "# ///");
            if let Some(close) = closing {
                if (i + 1..close).any(|n| opener(ls[n].text).is_some()) {
                    return Err(error(
                        "ambiguous_script",
                        "nested metadata starts are unsupported",
                        4,
                    ));
                }
                if kind == "script" {
                    if selected.is_some() {
                        return Err(error(
                            "ambiguous_script",
                            "multiple script metadata blocks",
                            4,
                        ));
                    }
                    selected = Some((i, close));
                }
                i = close + 1;
                continue;
            } else if kind == "script" {
                unclosed_script = true;
            }
        }
        i += 1;
    }
    if let Some((open, close)) = selected {
        let mut toml = String::new();
        let mut raw_lines = Vec::new();
        for l in &ls[open + 1..close] {
            toml.push_str(content(l.text).unwrap());
            toml.push_str(&l.raw[l.text.len()..]);
            raw_lines.push(l.raw.to_owned());
        }
        return Ok(Block {
            toml,
            span: bom + ls[open].start + ls[open].raw.len()..bom + ls[close].start,
            raw_lines,
            created: false,
            newline: edit::newline(body),
        });
    }
    if !create {
        return Err(error(
            "missing_script",
            "script metadata block is missing",
            4,
        ));
    }
    if unclosed_script {
        return Err(error(
            "ambiguous_script",
            "cannot scaffold around an unclosed script block",
            4,
        ));
    }
    let preamble_lines = cookie_line.map_or(
        usize::from(ls.first().is_some_and(|l| l.text.starts_with("#!"))),
        |n| n + 1,
    );
    let at = bom
        + ls.iter()
            .take(preamble_lines)
            .map(|l| l.raw.len())
            .sum::<usize>();
    Ok(Block {
        toml: String::new(),
        span: at..at,
        raw_lines: Vec::new(),
        created: true,
        newline: edit::newline(body),
    })
}

impl Block {
    pub fn render(&self, source: &str, toml: &str) -> Result<String, PatchError> {
        parse(toml)?;
        if !self.created && toml == self.toml {
            return Ok(source.to_owned());
        }
        let mut embedded = String::new();
        let mut old_line = 0;
        for change in similar::TextDiff::configure()
            .timeout(std::time::Duration::from_millis(100))
            .diff_lines(&self.toml, toml)
            .iter_all_changes()
        {
            match change.tag() {
                similar::ChangeTag::Equal => {
                    embedded.push_str(&self.raw_lines[old_line]);
                    old_line += 1;
                }
                similar::ChangeTag::Delete => old_line += 1,
                similar::ChangeTag::Insert => {
                    let value = change.value();
                    embedded.push_str(if value.trim_end_matches(['\r', '\n']).is_empty() {
                        "#"
                    } else {
                        "# "
                    });
                    embedded.push_str(value);
                    if !value.ends_with('\n') {
                        embedded.push_str(self.newline);
                    }
                }
            }
        }
        if self.created {
            let prefix = if self.span.start > 0
                && !source[..self.span.start].ends_with(['\n', '\u{feff}'])
            {
                self.newline
            } else {
                ""
            };
            let suffix = if self.span.start < source.len() || source.ends_with('\n') {
                self.newline
            } else {
                ""
            };
            embedded = format!(
                "{prefix}# /// script{}{embedded}# ///{suffix}",
                self.newline
            );
        }
        let mut result = source.to_owned();
        result.replace_range(self.span.clone(), &embedded);
        // Re-discovery validates prefix mapping, delimiter precedence, and metadata shape.
        let check = extract(&result, false)?;
        let expected = parse(toml)?.into_mut();
        let actual = parse(&check.toml)?.into_mut();
        if expected.to_string() != actual.to_string() {
            return Err(edit::layout());
        }
        Ok(result)
    }
}

pub(crate) fn validate(doc: &Document<&str>) -> Result<(), PatchError> {
    if let Some(deps) = doc.get("dependencies") {
        let array = deps.as_array().ok_or_else(|| {
            error(
                "invalid_metadata",
                "script dependencies must be an array of strings",
                3,
            )
        })?;
        for dep in array.iter() {
            let text = dep.as_str().ok_or_else(|| {
                error(
                    "invalid_metadata",
                    "script dependencies must be an array of strings",
                    3,
                )
            })?;
            Requirement::<pep508_rs::VerbatimUrl>::from_str(text).map_err(|_| {
                error(
                    "invalid_metadata",
                    "script contains an invalid requirement",
                    3,
                )
            })?;
        }
    }
    if let Some(python) = doc.get("requires-python") {
        let text = python.as_str().ok_or_else(|| {
            error(
                "invalid_metadata",
                "requires-python must be a version-specifier string",
                3,
            )
        })?;
        VersionSpecifiers::from_str(text)
            .map_err(|_| error("invalid_metadata", "invalid requires-python specifiers", 3))?;
    }
    if doc.get("tool").is_some_and(|t| t.as_table_like().is_none()) {
        return Err(error("invalid_metadata", "script tool must be a table", 3));
    }
    Ok(())
}
