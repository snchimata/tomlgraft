use super::*;
use std::{
    ffi::CString,
    os::{
        fd::AsRawFd,
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
};

#[derive(Debug, PartialEq)]
pub(super) struct Snapshot {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
pub(super) fn is_alias(m: &fs::Metadata) -> bool {
    m.nlink() != 1
}
pub(super) fn validate_path(_: &Path) -> Result<(), PatchError> {
    Ok(())
}
pub(super) fn snapshot(path: &Path) -> Result<Snapshot, PatchError> {
    let m = fs::symlink_metadata(path).map_err(|_| io_error())?;
    let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| io_error())?;
    // Refuse ACLs/security labels/xattrs rather than silently dropping them.
    if unsafe { libc::listxattr(name.as_ptr(), std::ptr::null_mut(), 0) } != 0 {
        return Err(error(
            "unsupported_metadata",
            "extended attributes or ACLs require unsupported preservation",
            5,
        ));
    }
    Ok(Snapshot {
        dev: m.dev(),
        ino: m.ino(),
        mode: m.mode(),
        uid: m.uid(),
        gid: m.gid(),
        size: m.size(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    })
}
pub(super) fn qualify(path: &Path) -> Result<(), PatchError> {
    let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| io_error())?;
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::statfs(name.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(io_error());
    }
    let kind = unsafe { stat.assume_init() }.f_type;
    if kind != 0xef53 {
        return Err(error(
            "unsupported_filesystem",
            "writable support requires local ext4",
            5,
        ));
    }
    Ok(())
}
pub(super) fn secure(stage: &tempfile::NamedTempFile, s: &Snapshot) -> Result<(), PatchError> {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::fchown(stage.as_file().as_raw_fd(), s.uid, s.gid) } != 0 {
        return Err(error(
            "unsupported_metadata",
            "cannot preserve target ownership",
            5,
        ));
    }
    stage
        .as_file()
        .set_permissions(fs::Permissions::from_mode(s.mode & 0o7777))
        .map_err(|_| io_error())?;
    if snapshot(stage.path())?.mode != s.mode {
        return Err(error(
            "unsupported_metadata",
            "cannot preserve target permissions",
            5,
        ));
    }
    Ok(())
}
pub(super) fn sync_parent(parent: &Path) -> Result<Durability, PatchError> {
    File::open(parent).and_then(|f| f.sync_all()).map_err(|_| {
        error(
            "durability",
            "replacement committed but directory synchronization failed",
            1,
        )
    })?;
    Ok(Durability::Synced)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_extended_attributes_without_losing_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("attrs.toml");
        fs::write(&path, "x=1").unwrap();
        let name = CString::new(path.as_os_str().as_bytes()).unwrap();
        let attr = c"user.tomlgraft-test";
        assert_eq!(
            unsafe { libc::setxattr(name.as_ptr(), attr.as_ptr(), b"keep".as_ptr().cast(), 4, 0) },
            0
        );
        let op = Operation::Set {
            path: vec!["x".into()],
            value_toml: "2".into(),
        };
        assert_eq!(
            patch_file(&path, &[op], &Options::default())
                .unwrap_err()
                .report
                .error
                .as_ref()
                .unwrap()
                .code,
            "unsupported_metadata"
        );
        assert_eq!(fs::read(&path).unwrap(), b"x=1");
        assert_eq!(
            unsafe { libc::getxattr(name.as_ptr(), attr.as_ptr(), std::ptr::null_mut(), 0) },
            4
        );
    }
}
