//! OBJ reading: in-memory text is parsed as one parallel block; files and
//! other readers stream through blocks of whole lines (see [`block`]).
mod block;
mod chunk;
mod count;

mod stream;

use crate::obj::{Obj, ObjError, ReadOptions};
use std::{fs::File, io, io::Read, path::Path};

/// Parse OBJ text with the default options.
pub fn parse(text: &[u8]) -> Result<Obj, ObjError> {
    parse_with(text, ReadOptions::default())
}

/// Parse OBJ text held in memory, on all of `options.threads`.
pub fn parse_with(text: &[u8], options: ReadOptions) -> Result<Obj, ObjError> {
    let mut builder = block::Builder::new(options);
    builder.block(text)?;
    Ok(builder.finish())
}

/// Read an OBJ file with the default options.
pub fn read_file(path: impl AsRef<Path>) -> Result<Obj, ObjError> {
    read_file_with(path, ReadOptions::default())
}

/// Read an OBJ file: small files in one read, larger ones streamed so reading
/// overlaps parsing. A regular file alone: a device or a pipe is refused
/// before it is opened.
pub fn read_file_with(path: impl AsRef<Path>, options: ReadOptions) -> Result<Obj, ObjError> {
    let path = path.as_ref();
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a regular file").into());
    }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size <= options.block_bytes as u64 {
        let mut text = Vec::with_capacity(size as usize + 1);
        file.read_to_end(&mut text)?;
        return parse_with(&text, options);
    }
    read(file, options)
}

/// Read OBJ text from any reader, streaming in blocks of whole lines.
pub fn read<R: Read + Send>(reader: R, options: ReadOptions) -> Result<Obj, ObjError> {
    stream::read(reader, options)
}
