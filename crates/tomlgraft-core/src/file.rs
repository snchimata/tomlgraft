use crate::{
    Durability, Operation, Options, PatchError, Report, SOURCE_LIMIT, classify, error, patch_bytes,
    read_bounded,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(windows)]
#[path = "file_windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "file_linux.rs"]
mod platform;
#[cfg(not(any(windows, target_os = "linux")))]
#[path = "file_unsupported.rs"]
mod platform;

fn io_error() -> PatchError {
    error("io", "file transaction failed", 1)
}
fn conflict() -> PatchError {
    error(
        "external_change",
        "target changed during preparation; no retry was attempted",
        4,
    )
}

fn regular(path: &Path) -> Result<(), PatchError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| io_error())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || platform::is_alias(&metadata) {
        return Err(error(
            "unsupported_target",
            "target must be a regular file without aliases",
            5,
        ));
    }
    Ok(())
}

fn resolved(path: &Path) -> Result<PathBuf, PatchError> {
    let name = path
        .file_name()
        .ok_or_else(|| error("unsupported_target", "target requires a filename", 5))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(fs::canonicalize(parent).map_err(|_| io_error())?.join(name))
}

fn lock(path: &Path) -> Result<File, PatchError> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| error("unsupported_target", "target requires a UTF-8 filename", 5))?;
    let sidecar = path.with_file_name(format!(".tomlgraft.{name}.lock"));
    match fs::symlink_metadata(&sidecar) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() || platform::is_alias(&m) => {
            return Err(error(
                "unsupported_target",
                "lock sidecar is not a regular unaliased file",
                5,
            ));
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io_error()),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(sidecar).map_err(|_| io_error())?;
    file.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => {
            error("busy_lock", "another cooperating writer holds the lock", 4)
        }
        std::fs::TryLockError::Error(_) => {
            error("unsupported_lock", "filesystem locking is unavailable", 5)
        }
    })?;
    Ok(file)
}

pub fn patch_file(
    path: &Path,
    operations: &[Operation],
    options: &Options,
) -> Result<Report, PatchError> {
    transaction(path, operations, options, |_, _| Ok(()))
}

// Fault injection stays private and cannot be enabled through CLI/environment inputs.
fn transaction(
    path: &Path,
    operations: &[Operation],
    options: &Options,
    mut checkpoint: impl FnMut(&str, &Path) -> Result<(), PatchError>,
) -> Result<Report, PatchError> {
    let kind = classify(path, options.kind)?;
    let mut run = || {
        let path = resolved(path)?;
        platform::validate_path(&path)?;
        regular(&path)?;
        let preview = options.dry_run || options.check;
        let _lock = if preview { None } else { Some(lock(&path)?) };
        regular(&path)?;
        let input = read_bounded(File::open(&path).map_err(|_| io_error())?, SOURCE_LIMIT)?;
        let snapshot = if preview {
            None
        } else {
            Some(platform::snapshot(&path)?)
        };
        let mut prepared = patch_bytes(&input, kind, operations, options)?;
        prepared.report.target = Some(path.to_string_lossy().into_owned());
        if !prepared.report.changed.unwrap() {
            return Ok(prepared.report);
        }
        if preview {
            prepared.report.status = "would_change";
            return Ok(prepared.report);
        }
        let error_report = prepared.report.clone();
        let result = (|| {
            let parent = path.parent().unwrap();
            platform::qualify(&path)?;
            let mut stage = tempfile::Builder::new()
                .prefix(".tomlgraft-stage-")
                .tempfile_in(parent)
                .map_err(|_| io_error())?;
            let before_commit = (|| {
                platform::secure(&stage, snapshot.as_ref().unwrap())?;
                checkpoint("write", &path)?;
                stage.write_all(&prepared.output).map_err(|_| io_error())?;
                platform::secure(&stage, snapshot.as_ref().unwrap())?;
                checkpoint("sync", &path)?;
                stage.as_file().sync_all().map_err(|_| io_error())?;
                checkpoint("recheck", &path)?;
                regular(&path).map_err(|_| conflict())?;
                let now = read_bounded(File::open(&path).map_err(|_| conflict())?, SOURCE_LIMIT)
                    .map_err(|_| conflict())?;
                if now != input
                    || platform::snapshot(&path).map_err(|_| conflict())?
                        != *snapshot.as_ref().unwrap()
                {
                    return Err(conflict());
                }
                checkpoint("replace", &path)?;
                Ok(())
            })();
            if let Err(e) = before_commit {
                stage.close().map_err(|_| {
                    error(
                        "cleanup_failed",
                        "transaction failed and its owned tempfile could not be removed",
                        1,
                    )
                })?;
                return Err(e);
            }
            if let Err(e) = stage.persist(&path) {
                e.file.close().map_err(|_| {
                    error(
                        "cleanup_failed",
                        "replacement failed and its owned tempfile could not be removed",
                        1,
                    )
                })?;
                return Err(io_error());
            }
            prepared.report.committed = true;
            let synced =
                checkpoint("directory_sync", &path).and_then(|_| platform::sync_parent(parent));
            match synced {
                Ok(durability) => {
                    prepared.report.durability = durability;
                    Ok(prepared.report)
                }
                Err(mut e) => {
                    prepared.report.status = "error";
                    prepared.report.durability = Durability::Uncertain;
                    prepared.report.error = e.report.error.take();
                    e.report = Box::new(prepared.report);
                    Err(e)
                }
            }
        })();
        result.map_err(|mut e: PatchError| {
            if !e.report.committed {
                let detail = e.report.error.take();
                e.report = Box::new(error_report);
                e.report.status = "error";
                e.report.error = detail;
            }
            e
        })
    };
    run().map_err(|mut e| {
        if e.report.target.is_none() {
            e.report.target = Some(path.to_string_lossy().into_owned());
        }
        if e.report.target_kind.is_none() {
            e.report.target_kind = Some(kind);
        }
        e
    })
}

pub fn check_file(path: &Path, options: &Options) -> Result<Report, PatchError> {
    patch_file(
        path,
        &[],
        &Options {
            dry_run: true,
            create_script_block: false,
            ..options.clone()
        },
    )
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;
    fn operation() -> Operation {
        Operation::Set {
            path: vec!["x".into()],
            value_toml: "2".into(),
        }
    }
    #[test]
    fn faults_never_truncate_and_post_commit_errors_are_explicit() {
        for fault in ["write", "sync", "replace", "directory_sync"] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("a.toml");
            fs::write(&p, b"x = 1\n").unwrap();
            let e = transaction(&p, &[operation()], &Options::default(), |step, _| {
                if step == fault {
                    Err(io_error())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert_eq!(
                fs::read(&p).unwrap(),
                if fault == "directory_sync" {
                    b"x = 2\n"
                } else {
                    b"x = 1\n"
                }
            );
            assert_eq!(e.report.committed, fault == "directory_sync");
            assert_eq!(e.report.changed, Some(true));
            assert_eq!(e.report.operations, 1);
            assert_eq!(fs::read_dir(d.path()).unwrap().count(), 2);
            if e.report.committed {
                assert_eq!(e.report.durability, Durability::Uncertain);
            }
        }
    }
    #[test]
    fn detects_external_edits_and_replacement_identity() {
        for same_bytes in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("a.toml");
            fs::write(&p, "x = 1\n").unwrap();
            let e = transaction(&p, &[operation()], &Options::default(), |step, p| {
                if step == "recheck" {
                    if same_bytes {
                        let other = p.with_extension("new");
                        fs::write(&other, "x = 1\n").unwrap();
                        fs::remove_file(p).unwrap();
                        fs::rename(other, p).unwrap();
                    } else {
                        fs::write(p, "x = 9\n").unwrap();
                    }
                }
                Ok(())
            })
            .unwrap_err();
            assert_eq!(e.report.error.unwrap().code, "external_change");
            assert!(!e.report.committed);
        }
    }

    #[test]
    fn interrupted_writer_child() {
        let Some(path) = std::env::var_os("TOMLGRAFT_TEST_INTERRUPTION") else {
            return;
        };
        let p = PathBuf::from(path);
        let _ = transaction(&p, &[operation()], &Options::default(), |step, path| {
            if step == "recheck" {
                fs::write(path.with_extension("ready"), b"ready").unwrap();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            Ok(())
        });
    }

    #[test]
    fn interruption_keeps_original_and_releases_os_lock() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.toml");
        fs::write(&p, "x = 1\n").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "file::tests::interrupted_writer_child"])
            .env("TOMLGRAFT_TEST_INTERRUPTION", &p)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !p.with_extension("ready").exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready = p.with_extension("ready").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(ready, "child did not reach staged checkpoint");
        assert_eq!(fs::read(&p).unwrap(), b"x = 1\n");
        assert!(
            patch_file(&p, &[operation()], &Options::default())
                .unwrap()
                .committed
        );
        assert!(fs::read_dir(d.path()).unwrap().any(|f| {
            f.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".tomlgraft-stage-")
        }));
    }
}
