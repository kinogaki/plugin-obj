//! Wavefront material libraries (`.mtl`).
mod model;
mod read;
mod write;

pub use model::{MapOptions, Material, MaterialLibrary, TextureMap};
pub use read::{library_paths, parse, parse_with, read_file};
pub use write::to_bytes;

/// Write `library` to `path`, replacing any file there atomically.
pub fn write_file(
    path: impl AsRef<std::path::Path>,
    library: &MaterialLibrary,
) -> Result<(), crate::obj::ObjError> {
    crate::obj::atomic::replace(path.as_ref(), |file| {
        use std::io::Write;
        file.write_all(&to_bytes(library))?;
        Ok(())
    })
}
