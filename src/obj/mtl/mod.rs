//! Wavefront material libraries (`.mtl`).
mod model;
mod read;
mod write;

pub use model::{MapOptions, Material, MaterialLibrary, TextureMap};
pub use read::{library_paths, parse, read_file};
pub use write::to_bytes;

/// Write `library` to `path`, replacing any file there whole or not at all
/// (`files::replace`).
pub fn write_file(
    path: impl AsRef<std::path::Path>,
    library: &MaterialLibrary,
) -> Result<(), crate::obj::ObjError> {
    let bytes = to_bytes(library);
    kinogaki_studio_api::format::files::replace(path.as_ref(), |file| {
        std::io::Write::write_all(file, &bytes).map_err(crate::obj::ObjError::from)
    })
}
