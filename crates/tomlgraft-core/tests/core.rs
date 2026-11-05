use proptest::prelude::*;
use tomlgraft_core::*;
fn set(path: &[&str], value: &str) -> Operation {
    Operation::Set {
        path: path.iter().map(|s| (*s).into()).collect(),
        value_toml: value.into(),
    }
}
fn rm(path: &[&str]) -> Operation {
    Operation::Remove {
        path: path.iter().map(|s| (*s).into()).collect(),
    }
}
fn up(scope: Scope, requirement: &str) -> Operation {
    Operation::UpsertDependency {
        scope,
        requirement: requirement.into(),
    }
}
fn dep_rm(scope: Scope, selector: &str) -> Operation {
    Operation::RemoveDependency {
        scope,
        selector: selector.into(),
    }
}
fn patch(source: &str, kind: Kind, ops: &[Operation]) -> Result<PreparedPatch, PatchError> {
    patch_bytes(source.as_bytes(), kind, ops, &Options::default())
}
fn output(source: &str, kind: Kind, ops: &[Operation]) -> String {
    String::from_utf8(patch(source, kind, ops).unwrap().output).unwrap()
}
fn code(result: Result<PreparedPatch, PatchError>) -> &'static str {
    result.unwrap_err().report.error.unwrap().code
}

#[test]
fn local_replacements_preserve_comments_order_and_endings() {
    let fixture = include_str!("fixtures/locality.toml");
    assert_eq!(
        output(fixture, Kind::Pyproject, &[set(&["a.b", "x"], "17")]),
        fixture.replacen("0x10", "17", 1)
    );
    for nl in ["\n", "\r\n"] {
        let src = format!(
            "# untouched{nl}a.b = 0x10 # note{nl}a.c = 'keep'{nl}[tool.\"example.org\"]{nl}'ключ' = {{ x = 1, y = [2] }} # tail"
        );
        assert_eq!(
            output(&src, Kind::Toml, &[set(&["a", "b"], "17")]),
            src.replacen("0x10", "17", 1)
        );
        assert_eq!(
            output(
                &src,
                Kind::Toml,
                &[set(&["tool", "example.org", "ключ", "x"], "3")]
            ),
            src.replacen("x = 1", "x = 3", 1)
        );
        assert_eq!(output(&src, Kind::Toml, &[set(&["a", "b"], "16")]), src);
    }
    assert_eq!(
        output(
            "a=1979-05-27\nb=9223372036854775807",
            Kind::Toml,
            &[
                set(&["a"], "1979-05-27"),
                set(&["b"], "9_223_372_036_854_775_807")
            ]
        ),
        "a=1979-05-27\nb=9223372036854775807"
    );
}

#[test]
fn insert_and_remove_standard_and_inline_leaves() {
    let nested = "x = { a = { b = 1 }, z = 3 } # keep";
    let added = output(nested, Kind::Toml, &[set(&["x", "a", "c"], "2")]);
    assert!(added.starts_with("x = { a = { b = 1"));
    assert!(added.ends_with(", z = 3 } # keep"));
    let removed = output(&added, Kind::Toml, &[rm(&["x", "a", "b"])]);
    assert_eq!(
        toml_edit::Document::parse(&removed).unwrap()["x"]["a"]["c"].as_integer(),
        Some(2)
    );
    assert_eq!(output("a=1\nb=2", Kind::Toml, &[rm(&["b"])]), "a=1");
    assert_eq!(output("a=1\r\n", Kind::Toml, &[rm(&["a"])]), "\r\n");
    for source in [
        "",
        "# header",
        "x=1",
        "x=1\n",
        "[a]\nx=1\n# next\n[b]\ny=2\n",
    ] {
        let new = output(source, Kind::Toml, &[set(&["a", "z"], "true")]);
        let parsed = toml_edit::Document::parse(&new).unwrap();
        assert_eq!(parsed["a"]["z"].as_bool(), Some(true));
        assert_eq!(output(&new, Kind::Toml, &[set(&["a", "z"], "true")]), new);
    }
    assert_eq!(
        output(
            "# keep\na.b=1 # remove\na.c=2\n# end",
            Kind::Toml,
            &[rm(&["a", "b"])]
        ),
        "# keep\na.c=2\n# end"
    );
    let inserted = output(
        "x = { a = 1 } # keep\n",
        Kind::Toml,
        &[set(&["x", "b"], "2")],
    );
    assert!(inserted.ends_with(" # keep\n"));
    assert!(toml_edit::Document::parse(&inserted).unwrap()["x"]["b"].as_integer() == Some(2));
    let removed = output(&inserted, Kind::Toml, &[rm(&["x", "a"])]);
    assert!(
        toml_edit::Document::parse(&removed).unwrap()["x"]
            .as_inline_table()
            .unwrap()
            .get("a")
            .is_none()
    );
    assert_eq!(
        code(patch(
            "x={a=1}",
            Kind::Toml,
            &[set(&["x", "missing", "b"], "2")]
        )),
        "unsupported_layout"
    );
    assert_eq!(
        code(patch("x=1", Kind::Toml, &[set(&["x", "b"], "2")])),
        "type_conflict"
    );
    assert_eq!(
        code(patch("[a]\nx=1", Kind::Toml, &[rm(&["a"])])),
        "unsupported_structure"
    );
    assert_eq!(
        code(patch("[[a]]\nx=1", Kind::Toml, &[set(&["a", "x"], "2")])),
        "type_conflict"
    );
}

#[test]
fn ordered_operations_refresh_spans_and_fail_atomically() {
    assert_eq!(
        output(
            "a=1\nb=2",
            Kind::Toml,
            &[set(&["a"], "10000"), set(&["b"], "3"), rm(&["a"])]
        ),
        "b=3"
    );
    for index in 0..3 {
        let mut ops = vec![set(&["a"], "2"); 3];
        ops[index] = rm(&["missing"]);
        let e = patch("a=1", Kind::Toml, &ops).unwrap_err();
        assert_eq!(e.report.error.unwrap().operation, Some(index));
        assert_eq!(e.report.operations, 0);
        assert!(!e.report.committed);
        assert_eq!(e.report.changed, None);
    }
    assert_eq!(
        code(patch("a=1", Kind::Toml, &[set(&["a"], "1\nx=2")])),
        "invalid_value"
    );
    assert_eq!(code(patch("bad = [", Kind::Toml, &[])), "invalid_document");
}

#[test]
fn requirement_identity_and_noop_retain_representation() {
    let marker_source = "[project]\ndependencies=['x; python_version >= \"0\"']";
    let absent = output(marker_source, Kind::Pyproject, &[up(Scope::Project, "x")]);
    assert_eq!(
        toml_edit::Document::parse(&absent).unwrap()["project"]["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let url_source = "[project]\ndependencies=['x @ https://example.org/a;part.whl']";
    assert_eq!(
        output(
            url_source,
            Kind::Pyproject,
            &[up(Scope::Project, "x @ https://example.org/a;part.whl")]
        ),
        url_source
    );
    let src = "[project]\ndependencies = [\n    'My_Pkg[B,a] >= 1', # associated\n    \"my-pkg; python_version < '3.12'\",\n    'my-pkg',\n]\n# tail\n";
    let result = output(
        src,
        Kind::Pyproject,
        &[up(Scope::Project, "my-pkg[a,b]>=2")],
    );
    assert_eq!(
        result,
        src.replacen("My_Pkg[B,a] >= 1", "my-pkg[a,b]>=2", 1)
    );
    assert_eq!(
        output(
            src,
            Kind::Pyproject,
            &[up(Scope::Project, "MY-PKG[a,b]>=1")]
        ),
        src
    );
    let removed = output(src, Kind::Pyproject, &[dep_rm(Scope::Project, "my-pkg")]);
    assert!(removed.contains("associated"));
    assert!(!removed.contains("    'my-pkg',"));
    assert_eq!(
        code(patch(
            "[project]\ndependencies=['x','X>=1']",
            Kind::Pyproject,
            &[up(Scope::Project, "x>=2")]
        )),
        "ambiguous_dependency"
    );
    assert_eq!(
        code(patch(
            src,
            Kind::Pyproject,
            &[dep_rm(Scope::Project, "my-pkg>=2")]
        )),
        "invalid_selector"
    );
    assert_eq!(
        code(patch(src, Kind::Toml, &[up(Scope::Project, "x")])),
        "invalid_scope"
    );
    let url = "x @ https://example.org/x.whl#sha256=abc";
    let updated = output(
        "[project]\ndependencies=['x']",
        Kind::Pyproject,
        &[up(Scope::Project, url)],
    );
    assert!(updated.contains(url));
}

#[test]
fn dependency_insert_remove_styles_and_comments() {
    for array in [
        "[]",
        "['x']",
        "['x',]",
        "[ 'x', 'y' ]",
        "[\n    'x' # own\n]",
        "[\n # standalone\n    'x', # own\n    'y',\n]",
    ] {
        let source = format!("[project]\ndependencies={array}\n# outside");
        let added = output(&source, Kind::Pyproject, &[up(Scope::Project, "z>=1")]);
        assert!(added.ends_with("\n# outside"));
        assert_eq!(
            output(&added, Kind::Pyproject, &[up(Scope::Project, "z>=1")]),
            added
        );
        let removed = output(&added, Kind::Pyproject, &[dep_rm(Scope::Project, "z")]);
        assert!(!removed.contains("z>=1"));
        if source.contains("standalone") {
            assert!(removed.contains("standalone"));
        }
    }
    assert_eq!(
        output(
            "[project]\ndependencies=['x',]",
            Kind::Pyproject,
            &[dep_rm(Scope::Project, "x")]
        ),
        "[project]\ndependencies=[]"
    );
}

#[test]
fn groups_optional_and_dynamic_guards_apply_to_generic_operations() {
    let src = "[dependency-groups]\nDev_Test=['x', { include-group = 'base' }]\nbase=['y']\n";
    let result = output(
        src,
        Kind::Pyproject,
        &[up(
            Scope::Group {
                name: "dev-test".into(),
            },
            "z",
        )],
    );
    assert!(result.contains("Dev_Test"));
    assert!(result.contains("{ include-group = 'base' }"));
    assert_eq!(
        code(patch(
            "[dependency-groups]\na=['x']\nA=['y']",
            Kind::Pyproject,
            &[up(Scope::Group { name: "a".into() }, "z")]
        )),
        "ambiguous_name"
    );
    assert_eq!(
        code(patch(
            "[dependency-groups]\na=[{ include-group='base', unknown=1 }]",
            Kind::Pyproject,
            &[up(Scope::Group { name: "a".into() }, "z")]
        )),
        "unsupported_metadata"
    );
    assert_eq!(
        code(patch(
            "[project]\ndynamic=['dependencies']",
            Kind::Pyproject,
            &[up(Scope::Project, "x")]
        )),
        "dynamic_field"
    );
    assert_eq!(
        code(patch(
            "[project]\ndynamic=['version']\nversion='1'",
            Kind::Pyproject,
            &[set(&["project", "version"], "'2'")]
        )),
        "dynamic_field"
    );
    let result = output(
        "[project]\ndynamic=['version']\nversion='1'",
        Kind::Pyproject,
        &[
            set(&["project", "version"], "'2'"),
            rm(&["project", "dynamic"]),
        ],
    );
    assert!(result.contains("version='2'"));
    assert_eq!(
        code(patch(
            "[project]",
            Kind::Pyproject,
            &[set(&["project", "dependencies"], "[1]")]
        )),
        "invalid_metadata"
    );
    let optional = output(
        "",
        Kind::Pyproject,
        &[up(
            Scope::Optional {
                name: "test".into(),
            },
            "x",
        )],
    );
    assert!(
        toml_edit::Document::parse(&optional).unwrap()["project"]["optional-dependencies"]["test"]
            .is_array()
    );
}

#[test]
fn lockfiles_are_read_only_from_bytes_and_paths() {
    let e = patch(
        "lock-version='1.0'\ncreated-by='x'\npackages=[]",
        Kind::Toml,
        &[set(&["created-by"], "'x'")],
    )
    .unwrap_err();
    assert_eq!(e.report.target_kind, Some(Kind::Lock));
    let lock =
        "lock-version='1.0'\ncreated-by='test'\n[[packages]]\nname='x'\n[[packages]]\nname='x'";
    assert_eq!(
        patch(lock, Kind::Toml, &[]).unwrap().report.validation,
        Some(Validation::LockStructure)
    );
    assert_eq!(
        code(patch(lock, Kind::Toml, &[set(&["created-by"], "'other'")])),
        "read_only_lock"
    );
    for source in [
        "lock-version='2.0'\ncreated-by='x'\npackages=[]",
        "lock-version='1.0'\ncreated-by='x'\n[[packages]]\nname='Bad_Name'",
        "lock-version='1.0'\ncreated-by='x'\npackages=['x']",
    ] {
        assert_eq!(code(patch(source, Kind::Lock, &[])), "invalid_lock");
    }
    for name in ["pylock.toml", "pylock.dev.toml", "PYLOCK.TOML"] {
        assert!(classify(std::path::Path::new(name), Some(Kind::Toml)).is_err());
    }
}

#[test]
fn scripts_preserve_prefixes_preamble_code_and_opaque_blocks() {
    for nl in ["\n", "\r\n"] {
        let src = format!(
            "\u{feff}#!/usr/bin/python{nl}# coding: utf-8{nl}# /// opaque{nl}# not toml{nl}# ///{nl}{nl}# /// script{nl}#{nl}# dependencies = ['x'] # keep{nl}# ///{nl}print('keep')"
        );
        let out = output(&src, Kind::Script, &[up(Scope::Script, "x>=1")]);
        assert_eq!(out, src.replacen("['x']", "['x>=1']", 1));
        assert_eq!(output(&src, Kind::Script, &[]), src);
    }
    let string = "s = '''\n# /// script\n# dependencies=[]\n# ///\n'''";
    assert!(output(string, Kind::Script, &[up(Scope::Script, "x")]).starts_with("s = '''\n"));
    assert_eq!(
        code(patch("# coding: latin-1\nprint(1)", Kind::Script, &[])),
        "unsupported_encoding"
    );
    assert_eq!(
        code(patch(
            "# /// script\n# dependencies=[1]\n# ///",
            Kind::Script,
            &[]
        )),
        "invalid_metadata"
    );
    assert_eq!(
        code(patch(
            "# /// script\n# requires-python='bad'\n# ///",
            Kind::Script,
            &[]
        )),
        "invalid_metadata"
    );
}

#[test]
fn script_discovery_and_explicit_creation() {
    for src in [
        "# /// script\n# dependencies=[]\n# ///\ncode\n# /// script\n# ///",
        "# /// script\n# /// other\n# ///",
    ] {
        assert_eq!(code(patch(src, Kind::Script, &[])), "ambiguous_script");
    }
    let close_precedence =
        "# /// script\n# text = '''\n# ///\n# still text\n# '''\n# ///\nprint(1)";
    assert_eq!(
        output(close_precedence, Kind::Script, &[]),
        close_precedence
    );
    let opts = Options {
        create_script_block: true,
        ..Options::default()
    };
    for source in [
        "",
        "print(1)",
        "#!/usr/bin/python\n# coding: utf-8\nprint(1)",
        "\u{feff}# coding=utf8\r\nprint(1)\r\n",
    ] {
        let out = patch_bytes(
            source.as_bytes(),
            Kind::Script,
            &[up(Scope::Script, "rich")],
            &opts,
        )
        .unwrap();
        let out = String::from_utf8(out.output).unwrap();
        assert!(out.contains("# /// script"));
        assert!(out.contains("print(1)") || source.is_empty());
        if source.contains("coding") {
            assert!(out.find("coding").unwrap() < out.find("# /// script").unwrap());
        }
        assert_eq!(out.ends_with('\n'), source.ends_with('\n'));
        assert_eq!(
            output(&out, Kind::Script, &[up(Scope::Script, "rich")]),
            out
        );
    }
    assert_eq!(
        code(patch_bytes(
            b"# /// script\n# x=1",
            Kind::Script,
            &[up(Scope::Script, "x")],
            &opts
        )),
        "ambiguous_script"
    );
    assert_eq!(
        code(patch_bytes(
            b"print(1)",
            Kind::Script,
            &[rm(&["dependencies"])],
            &opts
        )),
        "missing_script"
    );
}

#[test]
fn wire_rejects_unknown_fields_versions_and_limits() {
    for json in [
        r#"{"schema_version":2,"operations":[]}"#,
        r#"{"schema_version":1,"operations":[],"extra":1}"#,
        r#"{"schema_version":1,"operations":[{"op":"remove","path":["x"],"extra":1}]}"#,
        r#"{"schema_version":1,"operations":[{"op":"dep_upsert","scope":{"kind":"project","name":"x"},"requirement":"x"}]}"#,
    ] {
        assert!(decode_batch(json.as_bytes()).is_err());
    }
    assert!(decode_batch(&vec![b' '; BATCH_LIMIT + 1]).is_err());
    assert!(
        patch_bytes(
            &vec![b' '; SOURCE_LIMIT + 1],
            Kind::Toml,
            &[],
            &Options::default()
        )
        .is_err()
    );
    assert!(patch_bytes(&[0xff], Kind::Toml, &[], &Options::default()).is_err());
    assert_eq!(
        parse_path("tool.\"example.org\".enabled").unwrap(),
        vec!["tool", "example.org", "enabled"]
    );
    assert!(parse_path("bad..path").is_err());
}

proptest! {
    #[test]
    fn generated_noops_keep_bytes(n in any::<i64>(), text in "[a-zA-Z0-9 #_-]{0,80}", crlf in any::<bool>(), final_nl in any::<bool>()) {
        let nl=if crlf {"\r\n"} else {"\n"};
        let source=format!("# {text}{nl}'ключ' = {n} # keep{}",if final_nl {nl} else {""});
        prop_assert_eq!(output(&source,Kind::Toml,&[]),source.clone());
        prop_assert_eq!(output(&source,Kind::Toml,&[set(&["ключ"],&n.to_string())]),source);
    }
}
