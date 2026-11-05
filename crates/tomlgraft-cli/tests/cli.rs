#![cfg(any(windows, target_os = "linux"))]
use serde_json::Value;
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    process::{Command, Stdio},
};
fn run(args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_tomlgraft"))
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let json =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("invalid JSON: {:?}", out));
    (out.status.code().unwrap(), json)
}

#[test]
fn preview_noop_commit_and_check_have_distinct_reports() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.toml");
    fs::write(&p, "x = 1 # keep\n").unwrap();
    let path = p.to_str().unwrap();
    let modified = fs::metadata(&p).unwrap().modified().unwrap();
    let (exit, report) = run(&["set", path, "x", "2", "--dry-run", "--diff"]);
    assert_eq!(exit, 0);
    assert_eq!(report["status"], "would_change");
    assert_eq!(report["committed"], false);
    assert!(report["diff"].as_str().unwrap().contains("+x = 2 # keep"));
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    assert_eq!(run(&["set", path, "x", "2", "--check"]).0, 6);
    assert_eq!(run(&["set", path, "x", "1"]).1["status"], "unchanged");
    assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), modified);
    let (exit, report) = run(&["set", path, "x", "2"]);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["committed"], true);
    assert_eq!(report["operations"], 1);
    assert_eq!(fs::read_to_string(&p).unwrap(), "x = 2 # keep\n");
    assert_eq!(run(&["check", path]).1["validation"], "toml_syntax");
}

#[test]
fn stdin_batch_rolls_back_and_errors_are_safe_json() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("pyproject.toml");
    fs::write(&p, "[project]\nversion='1'\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tomlgraft"))
        .args([
            "batch",
            p.to_str().unwrap(),
            "--patch",
            "-",
            "--format",
            "json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"schema_version":1,"operations":[{"op":"set","path":["project","version"],"value_toml":"'2'"},{"op":"remove","path":["missing"]}]}"#).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["error"]["operation"], 1);
    assert_eq!(report["operations"], 0);
    assert_eq!(fs::read_to_string(&p).unwrap(), "[project]\nversion='1'\n");
    let (exit, report) = run(&["set", p.to_str().unwrap(), "project.version", "SECRET=bad"]);
    assert_eq!(exit, 2);
    assert!(!report.to_string().contains("SECRET"));
    assert_eq!(
        run(&["check", d.path().join("missing.toml").to_str().unwrap()]).0,
        1
    );
    let invalid = d.path().join("invalid.toml");
    fs::write(&invalid, "password=SECRET invalid").unwrap();
    let (exit, report) = run(&["check", invalid.to_str().unwrap()]);
    assert_eq!(exit, 3);
    assert!(!report.to_string().contains("SECRET"));
}

#[test]
fn lock_guard_and_kind_override_cannot_be_bypassed() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("pylock.dev.toml");
    fs::write(&p, "lock-version='1.0'\ncreated-by='test'\npackages=[]").unwrap();
    assert_eq!(
        run(&["check", p.to_str().unwrap()]).1["validation"],
        "lock_structure"
    );
    assert_eq!(run(&["set", p.to_str().unwrap(), "created-by", "'x'"]).0, 5);
    assert_eq!(
        run(&[
            "set",
            p.to_str().unwrap(),
            "created-by",
            "'x'",
            "--kind",
            "toml"
        ])
        .0,
        5
    );
    let generic = d.path().join("a.toml");
    fs::copy(&p, &generic).unwrap();
    assert_eq!(
        run(&["set", generic.to_str().unwrap(), "created-by", "'x'"]).0,
        5
    );
}

#[test]
fn independent_process_respects_sidecar_and_dry_run_ignores_it() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.toml");
    fs::write(&p, "x=1\n").unwrap();
    let sidecar = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(d.path().join(".tomlgraft.a.toml.lock"))
        .unwrap();
    sidecar.try_lock().unwrap();
    let (exit, report) = run(&["set", p.to_str().unwrap(), "x", "2"]);
    assert_eq!(exit, 4);
    assert_eq!(report["error"]["code"], "busy_lock");
    assert_eq!(
        run(&["set", p.to_str().unwrap(), "x", "2", "--dry-run"]).0,
        0
    );
    drop(sidecar);
    assert_eq!(run(&["set", p.to_str().unwrap(), "x", "2"]).0, 0);
}

#[test]
fn concurrent_successes_observe_previous_commit() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.toml");
    fs::write(&p, "a=0\nb=0\n").unwrap();
    let mut a = Command::new(env!("CARGO_BIN_EXE_tomlgraft"))
        .args(["set", p.to_str().unwrap(), "a", "1", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let b = run(&["set", p.to_str().unwrap(), "b", "1"]);
    let exit = a.wait().unwrap().code().unwrap();
    assert!([0, 4].contains(&exit));
    assert!([0, 4].contains(&b.0));
    assert!(exit == 0 || b.0 == 0);
    let bytes = fs::read_to_string(&p).unwrap();
    if exit == 0 {
        assert!(bytes.contains("a=1"));
    }
    if b.0 == 0 {
        assert!(bytes.contains("b=1"));
    }
}

#[test]
fn hardlinks_and_sidecar_links_refuse_without_data_loss() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.toml");
    let alias = d.path().join("alias.toml");
    fs::write(&p, "x=1").unwrap();
    fs::hard_link(&p, &alias).unwrap();
    assert_eq!(run(&["set", p.to_str().unwrap(), "x", "2"]).0, 5);
    assert_eq!(fs::read(&alias).unwrap(), b"x=1");
    #[cfg(unix)]
    {
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&p, d.path().join(".tomlgraft.a.toml.lock")).unwrap();
        assert_eq!(run(&["set", p.to_str().unwrap(), "x", "2"]).0, 5);
    }
}

#[test]
fn permissions_are_preserved_and_special_metadata_refuses() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.toml");
    fs::write(&p, "x=1").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(run(&["set", p.to_str().unwrap(), "x", "2"]).0, 0);
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    #[cfg(windows)]
    {
        let stream = format!("{}:private", p.display());
        fs::File::create(&stream)
            .unwrap()
            .write_all(b"keep")
            .unwrap();
        assert_eq!(run(&["set", p.to_str().unwrap(), "x", "2"]).0, 5);
        assert_eq!(fs::read(&stream).unwrap(), b"keep");
        assert_eq!(fs::read(&p).unwrap(), b"x=1");
    }
}
