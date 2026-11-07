//! Atomic file replacement: the text goes to a temporary file beside the
//! target, which is synced and renamed over it, so readers never see a
//! partial file and a failed write leaves the old one in place.
use crate::obj::ObjError;
use std::{fs::File, path::Path};

pub fn replace(
    path: &Path,
    write: impl FnOnce(&mut File) -> Result<(), ObjError>,
) -> Result<(), ObjError> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temporary = path.with_file_name(name);
    let result = (|| {
        let mut file = File::create(&temporary)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        // Make the rename itself durable.
        #[cfg(unix)]
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
