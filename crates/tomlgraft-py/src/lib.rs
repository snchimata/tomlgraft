use pyo3::{
    create_exception,
    exceptions::PyException,
    prelude::*,
    types::{PyBytes, PyDict},
};
use std::path::PathBuf;
use tomlgraft_core::{Kind, Operation, Options};

create_exception!(tomlgraft, PatchError, PyException);
create_exception!(tomlgraft, ArgumentError, PatchError);
create_exception!(tomlgraft, DocumentError, PatchError);
create_exception!(tomlgraft, ConflictError, PatchError);
create_exception!(tomlgraft, UnsupportedError, PatchError);
create_exception!(tomlgraft, TransactionError, PatchError);

fn json_object<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?.call_method1("loads", (text,))
}

fn py_error(py: Python<'_>, e: tomlgraft_core::PatchError) -> PyErr {
    let detail = e.report.error.as_ref().unwrap();
    let exception = match e.exit_code {
        2 => ArgumentError::new_err(detail.message),
        3 => DocumentError::new_err(detail.message),
        4 => ConflictError::new_err(detail.message),
        5 => UnsupportedError::new_err(detail.message),
        _ => TransactionError::new_err(detail.message),
    };
    let value = exception.value(py);
    let attributes = (|| -> PyResult<()> {
        value.setattr("code", detail.code)?;
        value.setattr("operation", detail.operation)?;
        value.setattr("committed", e.report.committed)?;
        value.setattr("exit_code", e.exit_code)?;
        let report = json_object(py, &serde_json::to_string(&e.report).unwrap())?;
        value.setattr("durability", report.get_item("durability")?)?;
        value.setattr("report", report)?;
        Ok(())
    })();
    match attributes {
        Ok(()) => exception,
        Err(e) => e,
    }
}

fn kind(py: Python<'_>, text: &str) -> PyResult<Kind> {
    serde_json::from_value(serde_json::Value::String(text.into())).map_err(|_| {
        py_error(
            py,
            tomlgraft_core::PatchError::new(
                "invalid_kind",
                "kind must be toml, pyproject, script, or lock",
                2,
            ),
        )
    })
}

fn operations(py: Python<'_>, ops: &Bound<'_, PyAny>) -> PyResult<Vec<Operation>> {
    let serialized: String = py
        .import("json")?
        .call_method1("dumps", (ops,))?
        .extract()?;
    let batch = format!("{{\"schema_version\":1,\"operations\":{serialized}}}");
    tomlgraft_core::decode_batch(batch.as_bytes()).map_err(|e| py_error(py, e))
}

#[pyfunction(signature = (source, *, kind, operations, create_script_block=false))]
fn patch<'py>(
    py: Python<'py>,
    source: &Bound<'py, PyBytes>,
    kind: &str,
    operations: &Bound<'py, PyAny>,
    create_script_block: bool,
) -> PyResult<Bound<'py, PyDict>> {
    if source.as_bytes().len() > tomlgraft_core::SOURCE_LIMIT {
        return Err(py_error(
            py,
            tomlgraft_core::PatchError::new("limit_exceeded", "source exceeds 16 MiB", 2),
        ));
    }
    let source = source.as_bytes().to_vec();
    let kind = self::kind(py, kind)?;
    let ops = self::operations(py, operations)?;
    let options = Options {
        create_script_block,
        ..Options::default()
    };
    let prepared = py
        .detach(move || tomlgraft_core::patch_bytes(&source, kind, &ops, &options))
        .map_err(|e| py_error(py, e))?;
    let result = PyDict::new(py);
    result.set_item("output", PyBytes::new(py, &prepared.output))?;
    result.set_item(
        "report",
        json_object(py, &serde_json::to_string(&prepared.report).unwrap())?,
    )?;
    Ok(result)
}

#[pyfunction(signature = (path, operations, *, kind=None, dry_run=false, create_script_block=false))]
fn patch_file<'py>(
    py: Python<'py>,
    path: PathBuf,
    operations: &Bound<'py, PyAny>,
    kind: Option<&str>,
    dry_run: bool,
    create_script_block: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let ops = self::operations(py, operations)?;
    let kind = kind.map(|s| self::kind(py, s)).transpose()?;
    let options = Options {
        kind,
        dry_run,
        create_script_block,
        ..Options::default()
    };
    let report = py
        .detach(move || tomlgraft_core::patch_file(&path, &ops, &options))
        .map_err(|e| py_error(py, e))?;
    json_object(py, &serde_json::to_string(&report).unwrap())
}

#[pymodule(gil_used = true)]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(patch, m)?)?;
    m.add_function(wrap_pyfunction!(patch_file, m)?)?;
    let py = m.py();
    m.add("PatchError", py.get_type::<PatchError>())?;
    m.add("ArgumentError", py.get_type::<ArgumentError>())?;
    m.add("DocumentError", py.get_type::<DocumentError>())?;
    m.add("ConflictError", py.get_type::<ConflictError>())?;
    m.add("UnsupportedError", py.get_type::<UnsupportedError>())?;
    m.add("TransactionError", py.get_type::<TransactionError>())?;
    Ok(())
}
