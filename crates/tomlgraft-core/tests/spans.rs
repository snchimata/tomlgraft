use toml_edit::{Document, Key, Value};

#[test]
fn parser_spans_locate_values_without_serializing_document() {
    let source = "# keep\r\na.b = 0x10 # note\r\nx = { 'a.b' = [1, 2] }\r\n[t]\r\nz = 'last'";
    let doc = Document::parse(source).unwrap();
    assert_eq!(&source[doc["a"]["b"].span().unwrap()], "0x10");
    assert_eq!(&source[doc["x"]["a.b"].span().unwrap()], "[1, 2]");
    assert_eq!(&source[doc["t"]["z"].span().unwrap()], "'last'");
    assert_eq!(
        Key::parse("tool.\"example.org\".enabled")
            .unwrap()
            .iter()
            .map(|k| k.get())
            .collect::<Vec<_>>(),
        ["tool", "example.org", "enabled"]
    );
    let mut output = source.to_string();
    output.replace_range(doc["a"]["b"].span().unwrap(), "17");
    assert_eq!(output, source.replacen("0x10", "17", 1));
    assert!(Document::parse(&output).is_ok());
    assert!("1\nx = 2".parse::<Value>().is_err());
    eprintln!(
        "table {:?}, key {:?}, value {:?}",
        doc["t"].span(),
        doc["t"].as_table().unwrap().key("z").unwrap().span(),
        doc["t"]["z"].span()
    );
}
