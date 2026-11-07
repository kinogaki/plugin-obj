//! Fast, bounded Wavefront OBJ and MTL reading and writing.
//!
//! [`Obj`] is a plain model of what an OBJ file holds: vertex tables,
//! polygons, polylines and point sets with their `vt`/`vn` references, and the
//! object, group, material and smoothing statements as runs over the elements.
//! It depends on no SDK; the plugin converts it to and from the standard
//! `Geometry`.
//!
//! Reading splits the text into chunks at line feeds and parses them in
//! parallel after a counting pass that sizes every table once; files stream
//! through blocks so reading overlaps parsing. Numbers parse bit-identically
//! to `str::parse::<f64>`, and writing uses the shortest spelling that reads
//! back exactly, so `parse(to_bytes(obj)) == obj`.
//!
//! ```
//! let obj = kinogaki_obj::obj::parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
//! assert_eq!(obj.faces.corners(0), &[0, 1, 2]);
//! let text = kinogaki_obj::obj::to_bytes(&obj).unwrap();
//! assert_eq!(kinogaki_obj::obj::parse(&text).unwrap(), obj);
//! ```
mod atomic;
mod error;
mod limits;
mod line;
mod model;
pub mod mtl;
mod number;
mod parallel;
mod read;
mod validate;
mod write;

pub use error::{Malformed, MalformedKind, ObjError};
pub use limits::{Limits, ReadOptions};
pub use model::{Elements, NONE, Obj, Run, Skipped, run_at};
pub use read::{parse, parse_with, read, read_file, read_file_with};
pub use write::{WriteOptions, to_bytes, write, write_file, write_with};
