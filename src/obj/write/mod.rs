//! OBJ writing. Tables and elements are formatted in parallel batches and
//! written in order, so memory holds a few megabytes of text at a time.
//! Numbers use the shortest spelling that reads back bit-identically, so
//! reading what was written gives back an equal [`Obj`].
mod format;

use crate::obj::{Elements, NONE, Obj, ObjError, Run, parallel};
pub(crate) use format::float;
use format::integer;
use std::{
    io::{self, Write},
    ops::Range,
    path::Path,
};

/// Records formatted per parallel task.
const PER_TASK: usize = 16 << 10;

/// How to write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Worker threads; 0 uses the available parallelism.
    pub threads: usize,
    /// A comment written first, one `#` line per line of text.
    pub comment: Option<String>,
}

/// Write `obj` as OBJ text with the default options.
pub fn write(obj: &Obj, out: impl Write) -> Result<(), ObjError> {
    write_with(obj, out, &WriteOptions::default())
}

/// The OBJ text for `obj`.
pub fn to_bytes(obj: &Obj) -> Result<Vec<u8>, ObjError> {
    let mut out = Vec::new();
    write(obj, &mut out)?;
    Ok(out)
}

/// Write `obj` to `path`, replacing any file there atomically: the text goes
/// to a temporary file beside it, which is synced and renamed over `path`.
pub fn write_file(
    path: impl AsRef<Path>,
    obj: &Obj,
    options: &WriteOptions,
) -> Result<(), ObjError> {
    crate::obj::atomic::replace(path.as_ref(), |file| write_with(obj, file, options))
}

/// Write `obj` as OBJ text.
pub fn write_with(obj: &Obj, mut out: impl Write, options: &WriteOptions) -> Result<(), ObjError> {
    obj.validate()
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let threads = crate::obj::limits::thread_count(options.threads);
    let mut text = Vec::new();
    if let Some(comment) = &options.comment {
        for line in comment.lines() {
            text.extend_from_slice(b"# ");
            text.extend_from_slice(line.as_bytes());
            text.push(b'\n');
        }
    }
    for library in &obj.material_libraries {
        statement(&mut text, b"mtllib", library);
    }
    out.write_all(&text)?;
    let out = &mut out;
    batches(out, obj.positions.len(), threads, |range, text| {
        vertices(obj, range, text)
    })?;
    batches(out, obj.texcoords.len(), threads, |range, text| {
        let mut buffer = kinogaki_float_text::FloatText::new();
        for &[u, v, w] in &obj.texcoords[range] {
            text.extend_from_slice(b"vt ");
            float(text, &mut buffer, u);
            text.push(b' ');
            float(text, &mut buffer, v);
            if w != 0.0 {
                text.push(b' ');
                float(text, &mut buffer, w);
            }
            text.push(b'\n');
        }
    })?;
    batches(out, obj.normals.len(), threads, |range, text| {
        let mut buffer = kinogaki_float_text::FloatText::new();
        for normal in &obj.normals[range] {
            text.extend_from_slice(b"vn");
            for &value in normal {
                text.push(b' ');
                float(text, &mut buffer, value);
            }
            text.push(b'\n');
        }
    })?;
    elements(obj, out, threads)
}

/// `v` records, with weights and colors when the model has them.
fn vertices(obj: &Obj, range: Range<usize>, text: &mut Vec<u8>) {
    let mut buffer = kinogaki_float_text::FloatText::new();
    for index in range {
        text.push(b'v');
        for &value in &obj.positions[index] {
            text.push(b' ');
            float(text, &mut buffer, value);
        }
        if let Some(&weight) = obj.weights.get(index) {
            text.push(b' ');
            float(text, &mut buffer, weight);
        }
        if let Some(color) = obj.colors.get(index) {
            for &value in color {
                text.push(b' ');
                float(text, &mut buffer, value);
            }
        }
        text.push(b'\n');
    }
}

/// Faces, polylines and point sets, with the `o`/`g`/`usemtl`/`s`
/// statements placed where their runs start.
fn elements(obj: &Obj, out: &mut impl Write, threads: usize) -> Result<(), ObjError> {
    let mut statements: Vec<((u32, u32, u32), Vec<u8>)> = Vec::new();
    let mut add = |runs: &[Run<String>], keyword: &[u8]| {
        for run in runs {
            let mut text = Vec::new();
            statement(&mut text, keyword, &run.value);
            statements.push(((run.face, run.line, run.point), text));
        }
    };
    add(&obj.objects, b"o");
    add(&obj.groups, b"g");
    add(&obj.materials, b"usemtl");
    for run in &obj.smoothing {
        let mut text = b"s ".to_vec();
        if run.value == 0 {
            text.extend_from_slice(b"off");
        } else {
            integer(&mut text, u64::from(run.value));
        }
        text.push(b'\n');
        statements.push(((run.face, run.line, run.point), text));
    }
    // Stable: statements at one place keep their kind order.
    statements.sort_by_key(|(at, _)| *at);
    let mut done = (0usize, 0usize, 0usize);
    let end = (
        obj.faces.len() as u32,
        obj.lines.len() as u32,
        obj.points.len() as u32,
    );
    for (at, text) in statements
        .iter()
        .map(|(at, text)| (*at, text.as_slice()))
        .chain([(end, &[][..])])
    {
        let until = (at.0 as usize, at.1 as usize, at.2 as usize);
        batches(out, until.0 - done.0, threads, |range, text| {
            rows(&obj.faces, b"f", shift(range, done.0), text)
        })?;
        batches(out, until.1 - done.1, threads, |range, text| {
            rows(&obj.lines, b"l", shift(range, done.1), text)
        })?;
        batches(out, until.2 - done.2, threads, |range, text| {
            rows(&obj.points, b"p", shift(range, done.2), text)
        })?;
        out.write_all(text)?;
        done = until;
    }
    Ok(())
}

fn shift(range: Range<usize>, by: usize) -> Range<usize> {
    range.start + by..range.end + by
}

/// One record per element: `keyword p`, `p/t`, `p//n` or `p/t/n` per corner.
fn rows(elements: &Elements, keyword: &[u8], range: Range<usize>, text: &mut Vec<u8>) {
    for element in range {
        text.extend_from_slice(keyword);
        for corner in elements.range(element) {
            text.push(b' ');
            integer(text, u64::from(elements.positions[corner]) + 1);
            let texcoord = elements.texcoords.get(corner).copied().unwrap_or(NONE);
            let normal = elements.normals.get(corner).copied().unwrap_or(NONE);
            if texcoord != NONE || normal != NONE {
                text.push(b'/');
                if texcoord != NONE {
                    integer(text, u64::from(texcoord) + 1);
                }
                if normal != NONE {
                    text.push(b'/');
                    integer(text, u64::from(normal) + 1);
                }
            }
        }
        text.push(b'\n');
    }
}

/// `keyword value` (just `keyword` for an empty value). A value ending in a
/// backslash gets an empty comment, so it does not read as a continuation.
fn statement(text: &mut Vec<u8>, keyword: &[u8], value: &str) {
    text.extend_from_slice(keyword);
    if !value.is_empty() {
        text.push(b' ');
        text.extend_from_slice(value.as_bytes());
    }
    if value.ends_with('\\') {
        text.extend_from_slice(b" #");
    }
    text.push(b'\n');
}

/// Format records `0..count` in parallel tasks of [`PER_TASK`] and write them
/// in order, a few tasks per thread at a time.
fn batches(
    out: &mut impl Write,
    count: usize,
    threads: usize,
    format: impl Fn(Range<usize>, &mut Vec<u8>) + Sync,
) -> Result<(), ObjError> {
    let batch = PER_TASK * threads * 2;
    let mut start = 0;
    while start < count {
        let stop = count.min(start + batch);
        let tasks: Vec<Range<usize>> = (start..stop)
            .step_by(PER_TASK)
            .map(|at| at..stop.min(at + PER_TASK))
            .collect();
        let texts = parallel::map(tasks, threads, |range| {
            let mut text = Vec::with_capacity(range.len() * 48);
            format(range, &mut text);
            text
        });
        for text in texts {
            out.write_all(&text)?;
        }
        start = stop;
    }
    Ok(())
}
