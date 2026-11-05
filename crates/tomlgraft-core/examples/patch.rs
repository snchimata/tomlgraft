use tomlgraft_core::{Kind, Operation, Options, Scope, patch_bytes};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = b"[project]\nversion = '1.0.0' # keep\ndependencies = ['requests>=2']\n";
    let operations = [
        Operation::Set {
            path: vec!["project".into(), "version".into()],
            value_toml: "'1.1.0'".into(),
        },
        Operation::UpsertDependency {
            scope: Scope::Project,
            requirement: "requests>=2.32".into(),
        },
    ];
    let prepared = patch_bytes(source, Kind::Pyproject, &operations, &Options::default())?;
    assert_eq!(
        prepared.output,
        b"[project]\nversion = '1.1.0' # keep\ndependencies = ['requests>=2.32']\n"
    );
    assert!(!prepared.report.committed);
    assert_eq!(
        patch_bytes(
            &prepared.output,
            Kind::Pyproject,
            &operations,
            &Options::default()
        )?
        .output,
        prepared.output
    );
    print!("{}", String::from_utf8(prepared.output)?);
    Ok(())
}
