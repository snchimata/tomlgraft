use super::*;
pub(super) type Snapshot = ();
pub(super) fn validate_path(_: &Path) -> Result<(), PatchError> {
    Ok(())
}
pub(super) fn is_alias(_: &fs::Metadata) -> bool {
    false
}
pub(super) fn snapshot(_: &Path) -> Result<Snapshot, PatchError> {
    Err(error(
        "unsupported_platform",
        "file writes are not qualified on this platform; use preview or patch_bytes",
        5,
    ))
}
pub(super) fn qualify(_: &Path) -> Result<(), PatchError> {
    snapshot(Path::new(""))
}
pub(super) fn secure(_: &tempfile::NamedTempFile, _: &Snapshot) -> Result<(), PatchError> {
    snapshot(Path::new(""))
}
pub(super) fn sync_parent(_: &Path) -> Result<Durability, PatchError> {
    Err(error(
        "unsupported_platform",
        "directory synchronization is unavailable",
        5,
    ))
}
