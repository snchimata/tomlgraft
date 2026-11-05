//! Byte-local TOML preparation and shared file transactions.
//!
//! ```
//! use tomlgraft_core::{patch_bytes, Kind, Operation, Options};
//! let operation = Operation::Set { path: vec!["x".into()], value_toml: "2".into() };
//! let prepared = patch_bytes(b"x=1 # keep", Kind::Toml, &[operation], &Options::default())?;
//! assert_eq!(prepared.output, b"x=2 # keep");
//! assert!(!prepared.report.committed);
//! # Ok::<(), tomlgraft_core::PatchError>(())
//! ```
mod dependency;
mod edit;
mod file;
mod script;

use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};
use toml_edit::{Document, Item};

pub use edit::parse_path;
pub use file::{check_file, patch_file};

pub const SOURCE_LIMIT: usize = 16 * 1024 * 1024;
pub const BATCH_LIMIT: usize = 1024 * 1024;
pub const OPERATION_LIMIT: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Toml,
    Pyproject,
    Script,
    Lock,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Project,
    Script,
    Optional { name: String },
    Group { name: String },
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum WireScope {
            Project {},
            Script {},
            Optional { name: String },
            Group { name: String },
        }
        Ok(match WireScope::deserialize(deserializer)? {
            WireScope::Project {} => Scope::Project,
            WireScope::Script {} => Scope::Script,
            WireScope::Optional { name } => Scope::Optional { name },
            WireScope::Group { name } => Scope::Group { name },
        })
    }
}

impl PatchError {
    pub fn new(code: &'static str, message: &'static str, exit_code: u8) -> Self {
        error(code, message, exit_code)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Set {
        path: Vec<String>,
        value_toml: String,
    },
    Remove {
        path: Vec<String>,
    },
    #[serde(rename = "dep_upsert")]
    UpsertDependency {
        scope: Scope,
        requirement: String,
    },
    #[serde(rename = "dep_remove")]
    RemoveDependency {
        scope: Scope,
        selector: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub schema_version: u32,
    pub operations: Vec<Operation>,
}

pub fn decode_batch(bytes: &[u8]) -> Result<Vec<Operation>, PatchError> {
    if bytes.len() > BATCH_LIMIT {
        return Err(error("limit_exceeded", "batch exceeds 1 MiB", 2));
    }
    let batch: Batch = serde_json::from_slice(bytes).map_err(|_| {
        error(
            "invalid_batch",
            "expected a versioned JSON batch with known fields and operations",
            2,
        )
    })?;
    if batch.schema_version != 1 {
        return Err(error("invalid_batch", "unsupported schema_version", 2));
    }
    if batch.operations.len() > OPERATION_LIMIT {
        return Err(error("limit_exceeded", "batch exceeds 1000 operations", 2));
    }
    Ok(batch.operations)
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub kind: Option<Kind>,
    pub create_script_block: bool,
    pub dry_run: bool,
    pub check: bool,
    pub diff: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Validation {
    TomlSyntax,
    SelectedMetadata,
    LockStructure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Durability {
    NotApplicable,
    Synced,
    Uncertain,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorDetail {
    pub code: &'static str,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub target: Option<String>,
    pub target_kind: Option<Kind>,
    pub status: &'static str,
    pub changed: Option<bool>,
    pub committed: bool,
    pub operations: usize,
    pub validation: Option<Validation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_omitted: Option<&'static str>,
    pub error: Option<ErrorDetail>,
    pub durability: Durability,
}

#[derive(Debug, Clone)]
pub struct PatchError {
    pub report: Box<Report>,
    pub exit_code: u8,
}
impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.report.error.as_ref().unwrap().message)
    }
}
impl std::error::Error for PatchError {}

pub(crate) fn error(code: &'static str, message: &'static str, exit_code: u8) -> PatchError {
    PatchError {
        exit_code,
        report: Box::new(Report {
            schema_version: 1,
            target: None,
            target_kind: None,
            status: "error",
            changed: None,
            committed: false,
            operations: 0,
            validation: None,
            diff: None,
            diff_omitted: None,
            error: Some(ErrorDetail {
                code,
                message,
                operation: None,
            }),
            durability: Durability::NotApplicable,
        }),
    }
}

#[derive(Debug)]
pub struct PreparedPatch {
    pub output: Vec<u8>,
    pub report: Report,
}

pub fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, PatchError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("io", "failed to read input", 1))?;
    if bytes.len() > limit {
        return Err(error("limit_exceeded", "input exceeds its size limit", 2));
    }
    Ok(bytes)
}

pub fn classify(path: &Path, requested: Option<Kind>) -> Result<Kind, PatchError> {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| error("unsupported_target", "target requires a UTF-8 filename", 5))?;
    let lower = name.to_ascii_lowercase();
    let named_lock = lower
        .strip_prefix("pylock.")
        .and_then(|s| s.strip_suffix(".toml"))
        .is_some_and(|s| !s.is_empty() && !s.contains('.'));
    let known = if lower == "pylock.toml" || named_lock {
        Some(Kind::Lock)
    } else if lower == "pyproject.toml" {
        Some(Kind::Pyproject)
    } else if lower.ends_with(".py") {
        Some(Kind::Script)
    } else {
        None
    };
    if let Some(kind) = known {
        if requested.is_some_and(|r| r != kind) {
            return Err(error(
                "kind_conflict",
                "kind cannot override a recognized target",
                5,
            ));
        }
        return Ok(kind);
    }
    requested
        .or_else(|| lower.ends_with(".toml").then_some(Kind::Toml))
        .ok_or_else(|| {
            error(
                "unsupported_target",
                "use --kind for an unrecognized filename",
                5,
            )
        })
}

pub(crate) fn parse(source: &str) -> Result<Document<&str>, PatchError> {
    Document::parse(source).map_err(|_| error("invalid_document", "input is not valid TOML", 3))
}

pub(crate) fn lock_validation(doc: &Document<&str>) -> Result<(), PatchError> {
    let valid = doc.get("lock-version").and_then(Item::as_str) == Some("1.0")
        && doc.get("created-by").and_then(Item::as_str).is_some()
        && doc.get("packages").is_some_and(|p| {
            if let Some(a) = p.as_array() {
                return a.is_empty();
            }
            p.as_array_of_tables().is_some_and(|a| {
                a.iter().all(|t| {
                    t.get("name")
                        .and_then(Item::as_str)
                        .is_some_and(|n| dependency::normalized_name(n).is_ok_and(|v| v == n))
                })
            })
        });
    if valid {
        Ok(())
    } else {
        Err(error(
            "invalid_lock",
            "lock_structure requires version 1.0, created-by, and packages with normalized names",
            3,
        ))
    }
}

pub fn patch_bytes(
    input: &[u8],
    kind: Kind,
    operations: &[Operation],
    options: &Options,
) -> Result<PreparedPatch, PatchError> {
    let result = prepare(input, kind, operations, options);
    result.map_err(|mut e| {
        if e.report.target_kind.is_none() {
            e.report.target_kind = Some(kind);
        }
        e
    })
}

fn prepare(
    input: &[u8],
    mut kind: Kind,
    operations: &[Operation],
    options: &Options,
) -> Result<PreparedPatch, PatchError> {
    if input.len() > SOURCE_LIMIT || operations.len() > OPERATION_LIMIT {
        return Err(error(
            "limit_exceeded",
            "source or operation count exceeds its limit",
            2,
        ));
    }
    let source = std::str::from_utf8(input)
        .map_err(|_| error("unsupported_encoding", "only UTF-8 input is supported", 5))?;
    let mut block = if kind == Kind::Script {
        Some(script::extract(
            source,
            options.create_script_block
                && !operations.is_empty()
                && operations.iter().all(|op| {
                    matches!(
                        op,
                        Operation::Set { .. } | Operation::UpsertDependency { .. }
                    )
                }),
        )?)
    } else {
        None
    };
    let original_toml = block.as_ref().map_or(source, |b| b.toml.as_str());
    let doc = parse(original_toml)?;
    if kind != Kind::Script && doc.contains_key("lock-version") {
        kind = Kind::Lock;
    }
    let validation = match kind {
        Kind::Lock => {
            lock_validation(&doc).map_err(|mut e| {
                e.report.target_kind = Some(Kind::Lock);
                e
            })?;
            Validation::LockStructure
        }
        Kind::Script => {
            script::validate(&doc)?;
            Validation::SelectedMetadata
        }
        _ => Validation::TomlSyntax,
    };
    if kind == Kind::Lock && !operations.is_empty() {
        let mut e = error(
            "read_only_lock",
            "lockfiles are read-only; regenerate with your chosen resolver",
            5,
        );
        e.report.target_kind = Some(Kind::Lock);
        return Err(e);
    }
    let mut working = original_toml.to_string();
    let mut touched = Vec::new();
    for (index, operation) in operations.iter().enumerate() {
        let step = (|| {
            let path = match operation {
                Operation::Set { path, value_toml } => {
                    dependency::validate_touched(&parse(&working)?, kind, path)?;
                    edit::set(&mut working, path, value_toml)?;
                    path.clone()
                }
                Operation::Remove { path } => {
                    dependency::validate_touched(&parse(&working)?, kind, path)?;
                    edit::remove(&mut working, path)?;
                    path.clone()
                }
                Operation::UpsertDependency { scope, requirement } => {
                    dependency::apply(&mut working, kind, scope, requirement, false)?
                }
                Operation::RemoveDependency { scope, selector } => {
                    dependency::apply(&mut working, kind, scope, selector, true)?
                }
            };
            if working.len() > SOURCE_LIMIT {
                return Err(error("limit_exceeded", "candidate exceeds 16 MiB", 2));
            }
            dependency::validate_touched(&parse(&working)?, kind, &path)?;
            touched.push((index, path));
            Ok(())
        })();
        if let Err(mut e) = step {
            e.report.error.as_mut().unwrap().operation = Some(index);
            return Err(e);
        }
    }
    let final_doc = parse(&working)?;
    if kind == Kind::Pyproject {
        dependency::check_dynamic(&final_doc, &touched)?;
    }
    if kind == Kind::Script {
        script::validate(&final_doc)?;
    }
    let output = if let Some(b) = block.as_mut() {
        b.render(source, &working)?
    } else {
        working
    };
    if output.len() > SOURCE_LIMIT {
        return Err(error("limit_exceeded", "candidate exceeds 16 MiB", 2));
    }
    let changed = output.as_bytes() != input;
    let mut report = Report {
        schema_version: 1,
        target: None,
        target_kind: Some(kind),
        status: if changed { "modified" } else { "unchanged" },
        changed: Some(changed),
        committed: false,
        operations: operations.len(),
        validation: Some(validation),
        diff: None,
        diff_omitted: None,
        error: None,
        durability: Durability::NotApplicable,
    };
    if options.diff {
        if input.len() + output.len() > 512 * 1024
            || source.lines().count() + output.lines().count() > 20_000
        {
            report.diff_omitted = Some("diff exceeds 512 KiB or 20000 lines");
        } else {
            report.diff = Some(
                similar::TextDiff::configure()
                    .timeout(std::time::Duration::from_millis(100))
                    .diff_lines(source, &output)
                    .unified_diff()
                    .header("before", "after")
                    .to_string(),
            );
        }
    }
    Ok(PreparedPatch {
        output: output.into_bytes(),
        report,
    })
}
