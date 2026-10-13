//! Private, non-clobbering local voice artifacts.
use std::{fs::File, io::Write, path::Path};
pub fn create_private(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = create_private(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn voice_artifacts_are_private_and_existing_recordings_are_never_truncated() {
        let path = std::env::temp_dir().join(format!("air-private-{}", uuid::Uuid::new_v4()));
        write_private(&path, b"original").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let overwrite = write_private(&path, b"replacement");
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(mode, 0o600);
        assert!(overwrite.is_err());
        assert_eq!(bytes, b"original");
    }
}
