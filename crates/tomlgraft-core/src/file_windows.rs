use super::*;
use std::os::windows::{ffi::OsStrExt, fs::MetadataExt, io::AsRawHandle};
use windows_sys::Win32::System::{
    SystemServices::{
        ACCESS_FILTER_SECURITY_INFORMATION, PROCESS_TRUST_LABEL_SECURITY_INFORMATION,
    },
    WindowsProgramming::DRIVE_FIXED,
};
use windows_sys::Win32::{Security::*, Storage::FileSystem::*};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
#[derive(Debug, PartialEq)]
pub(super) struct Snapshot {
    volume: u32,
    index: (u32, u32),
    attributes: u32,
    size: u64,
    modified: u64,
    security: Vec<u32>,
}
pub(super) fn is_alias(m: &fs::Metadata) -> bool {
    m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn security(path: &Path) -> Result<Vec<u32>, PatchError> {
    let path = wide(path);
    let mut needed = 0;
    let flags = OWNER_SECURITY_INFORMATION
        | GROUP_SECURITY_INFORMATION
        | DACL_SECURITY_INFORMATION
        | LABEL_SECURITY_INFORMATION
        | ATTRIBUTE_SECURITY_INFORMATION
        | SCOPE_SECURITY_INFORMATION
        | PROCESS_TRUST_LABEL_SECURITY_INFORMATION as u32
        | ACCESS_FILTER_SECURITY_INFORMATION as u32;
    unsafe {
        GetFileSecurityW(path.as_ptr(), flags, std::ptr::null_mut(), 0, &mut needed);
    }
    if needed == 0 || needed > 1024 * 1024 {
        return Err(error(
            "unsupported_metadata",
            "cannot read target security metadata",
            5,
        ));
    }
    let mut bytes = vec![0u32; (needed as usize).div_ceil(4)];
    if unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            flags,
            bytes.as_mut_ptr().cast(),
            bytes.len() as u32 * 4,
            &mut needed,
        )
    } == 0
    {
        return Err(error(
            "unsupported_metadata",
            "cannot read target security metadata",
            5,
        ));
    }
    let mut acl = std::ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    if unsafe {
        GetSecurityDescriptorSacl(bytes.as_ptr() as _, &mut present, &mut acl, &mut defaulted)
    } == 0
    {
        return Err(io_error());
    }
    if !acl.is_null() {
        for i in 0..unsafe { (*acl).AceCount } {
            let mut ace = std::ptr::null_mut();
            if unsafe { GetAce(acl, i as u32, &mut ace) } == 0 {
                return Err(io_error());
            }
            if unsafe { (*(ace as *const ACE_HEADER)).AceType } != 17
            /* SYSTEM_MANDATORY_LABEL_ACE_TYPE */
            {
                return Err(error(
                    "unsupported_metadata",
                    "Windows resource or access policy metadata is unsupported",
                    5,
                ));
            }
        }
    }
    Ok(bytes)
}

pub(super) fn validate_path(path: &Path) -> Result<(), PatchError> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(io_error)?;
    if name.contains(':') || name.trim_end_matches([' ', '.']) != name {
        return Err(error(
            "unsupported_target",
            "Windows alternate-stream and aliased paths are unsupported",
            5,
        ));
    }
    let p = wide(path);
    let mut buffer = vec![0u16; 32768];
    let length = unsafe { GetLongPathNameW(p.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        return Err(io_error());
    }
    let actual = PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize]));
    if !actual
        .file_name()
        .unwrap()
        .to_string_lossy()
        .eq_ignore_ascii_case(name)
    {
        return Err(error(
            "unsupported_target",
            "Windows short-name aliases are unsupported",
            5,
        ));
    }
    Ok(())
}
fn security_parts(bytes: &[u32]) -> Result<Vec<Vec<u8>>, PatchError> {
    let sd = bytes.as_ptr() as _;
    let mut parts = Vec::new();
    for get in [GetSecurityDescriptorOwner, GetSecurityDescriptorGroup] {
        let mut sid = std::ptr::null_mut();
        let mut defaulted = 0;
        if unsafe { get(sd, &mut sid, &mut defaulted) } == 0 || sid.is_null() {
            return Err(io_error());
        }
        parts.push(
            unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), GetLengthSid(sid) as usize) }
                .to_vec(),
        );
    }
    for (i, get) in [GetSecurityDescriptorDacl, GetSecurityDescriptorSacl]
        .into_iter()
        .enumerate()
    {
        let mut acl = std::ptr::null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        if unsafe { get(sd, &mut present, &mut acl, &mut defaulted) } == 0 {
            return Err(io_error());
        }
        parts.push(vec![if i == 1 && acl.is_null() {
            0
        } else {
            present as u8
        }]);
        parts.push(if acl.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), (*acl).AclSize as usize) }
                .to_vec()
        });
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) } == 0 {
        return Err(io_error());
    }
    parts.push((control & SE_DACL_PROTECTED).to_ne_bytes().to_vec());
    Ok(parts)
}
pub(super) fn snapshot(path: &Path) -> Result<Snapshot, PatchError> {
    let f = File::open(path).map_err(|_| io_error())?;
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(f.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(io_error());
    }
    let info = unsafe { info.assume_init() };
    if info.nNumberOfLinks != 1 {
        return Err(error(
            "unsupported_target",
            "hard-linked files cannot be atomically replaced",
            5,
        ));
    }
    let m = f.metadata().map_err(|_| io_error())?;
    Ok(Snapshot {
        volume: info.dwVolumeSerialNumber,
        index: (info.nFileIndexHigh, info.nFileIndexLow),
        attributes: m.file_attributes(),
        size: m.file_size(),
        modified: m.last_write_time(),
        security: security(path)?,
    })
}
pub(super) fn qualify(path: &Path) -> Result<(), PatchError> {
    let wide_path = wide(path);
    let mut root = vec![0u16; 32768];
    if unsafe { GetVolumePathNameW(wide_path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0
    {
        return Err(io_error());
    }
    let mut name = [0u16; 32];
    if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED
        || unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                name.as_mut_ptr(),
                name.len() as u32,
            )
        } == 0
        || String::from_utf16_lossy(
            &name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())],
        ) != "NTFS"
    {
        return Err(error(
            "unsupported_filesystem",
            "Windows writes require local fixed NTFS",
            5,
        ));
    }
    let attrs = fs::metadata(path)
        .map_err(|_| io_error())?
        .file_attributes();
    if attrs
        & !(FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_NORMAL | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED)
        != 0
    {
        return Err(error(
            "unsupported_metadata",
            "special Windows file attributes are unsupported for writes",
            5,
        ));
    }
    let mut stream = std::mem::MaybeUninit::<WIN32_FIND_STREAM_DATA>::uninit();
    let handle = unsafe {
        FindFirstStreamW(
            wide_path.as_ptr(),
            FindStreamInfoStandard,
            stream.as_mut_ptr().cast(),
            0,
        )
    };
    if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return Err(io_error());
    }
    let mut extra = false;
    loop {
        let data = unsafe { stream.assume_init_ref() };
        let end = data
            .cStreamName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(data.cStreamName.len());
        if String::from_utf16_lossy(&data.cStreamName[..end]) != "::$DATA" {
            extra = true;
        }
        if unsafe { FindNextStreamW(handle, stream.as_mut_ptr().cast()) } == 0 {
            break;
        }
    }
    let last = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    unsafe {
        FindClose(handle);
    }
    if last != windows_sys::Win32::Foundation::ERROR_HANDLE_EOF {
        return Err(io_error());
    }
    if extra {
        return Err(error(
            "unsupported_metadata",
            "alternate data streams are unsupported for writes",
            5,
        ));
    }
    Ok(())
}
pub(super) fn secure(stage: &tempfile::NamedTempFile, s: &Snapshot) -> Result<(), PatchError> {
    let path = wide(stage.path());
    let mut control = 0;
    let mut revision = 0;
    if unsafe {
        GetSecurityDescriptorControl(s.security.as_ptr() as _, &mut control, &mut revision)
    } == 0
    {
        return Err(io_error());
    }
    let protection = if control & SE_DACL_PROTECTED != 0 {
        PROTECTED_DACL_SECURITY_INFORMATION
    } else {
        UNPROTECTED_DACL_SECURITY_INFORMATION
    };
    let flags = OWNER_SECURITY_INFORMATION
        | GROUP_SECURITY_INFORMATION
        | DACL_SECURITY_INFORMATION
        | LABEL_SECURITY_INFORMATION
        | protection;
    if unsafe { SetFileSecurityW(path.as_ptr(), flags, s.security.as_ptr() as _) } == 0 {
        return Err(error(
            "unsupported_metadata",
            "cannot preserve target security metadata",
            5,
        ));
    }
    let actual = security_parts(&security(stage.path())?)?;
    let expected = security_parts(&s.security)?;
    if actual != expected {
        return Err(error(
            "unsupported_metadata",
            "staged security metadata differs from the target",
            5,
        ));
    }
    if unsafe { SetFileAttributesW(path.as_ptr(), s.attributes) } == 0 {
        return Err(io_error());
    }
    Ok(())
}
pub(super) fn sync_parent(_parent: &Path) -> Result<Durability, PatchError> {
    // Windows directory flush is not qualified; do not claim power-loss durability.
    Ok(Durability::Uncertain)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_preserves_protected_dacl_and_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("protected.toml");
        fs::write(&path, "x=1").unwrap();
        let original = security(&path).unwrap();
        let name = wide(&path);
        assert_ne!(
            unsafe {
                SetFileSecurityW(
                    name.as_ptr(),
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    original.as_ptr() as _,
                )
            },
            0
        );
        let expected = security_parts(&security(&path).unwrap()).unwrap();
        let op = Operation::Set {
            path: vec!["x".into()],
            value_toml: "2".into(),
        };
        assert!(
            patch_file(&path, &[op], &Options::default())
                .unwrap()
                .committed
        );
        assert_eq!(security_parts(&security(&path).unwrap()).unwrap(), expected);
    }
}
