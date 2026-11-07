//! The parsing pass over one chunk: vertex and face records go straight into
//! the chunk's slices of the shared tables; the rarer records (weights,
//! colors, polylines, point sets, state statements) collect in [`Extras`],
//! which the block merges in order afterwards.
use crate::obj::line::{Line, separator};
use crate::obj::{
    Elements, MalformedKind, NONE, Run,
    error::Fault,
    number::{delimiter, parse_index},
    options::CORNERS,
};
use std::collections::HashMap;

/// The chunk's parts of the tables, sized by the counting pass.
pub struct Tables<'t> {
    pub positions: &'t mut [[f64; 3]],
    pub texcoords: &'t mut [[f64; 3]],
    pub normals: &'t mut [[f64; 3]],
    /// Each face's end offset (global corner index).
    pub face_ends: &'t mut [u32],
    pub corner_positions: &'t mut [u32],
    /// Empty when the file has no `vt` (or `vn`) records so far.
    pub corner_texcoords: &'t mut [u32],
    pub corner_normals: &'t mut [u32],
}

/// Global counts of each table before this chunk.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bases {
    pub positions: usize,
    pub texcoords: usize,
    pub normals: usize,
    pub corners: usize,
}

/// Records kept outside the preallocated tables. Element counts in runs are
/// local to the chunk until merged.
#[derive(Default)]
pub struct Extras {
    pub weights: Vec<(u32, f64)>,
    pub colors: Vec<(u32, [f64; 3])>,
    pub lines: Elements,
    pub points: Elements,
    pub objects: Vec<Run<String>>,
    pub groups: Vec<Run<String>>,
    pub materials: Vec<Run<String>>,
    pub smoothing: Vec<Run<u32>>,
    pub libraries: Vec<String>,
    /// Keyword, count and the byte offset of its first statement.
    pub skipped: Vec<(Vec<u8>, u64, usize)>,
    pub face_texcoords: bool,
    pub face_normals: bool,
    pub line_texcoords: bool,
    pub fault: Option<Fault>,
}

/// One chunk to parse: `text[start..end]` of the block `text`.
pub struct Job<'t> {
    pub text: &'t [u8],
    pub start: usize,
    pub end: usize,
    pub tables: Tables<'t>,
    pub bases: Bases,
    /// The longest line read.
    pub line_bytes: usize,
}

pub fn parse(job: Job<'_>) -> Extras {
    let mut chunk = Chunk {
        text: job.text,
        tables: job.tables,
        bases: job.bases,
        line_bytes: job.line_bytes,
        skipped: HashMap::new(),
        positions: 0,
        texcoords: 0,
        normals: 0,
        faces: 0,
        corners: 0,
        extras: Extras::default(),
    };
    if let Err(fault) = chunk.lines(job.start, job.end) {
        chunk.extras.fault = Some(fault);
    }
    chunk.extras
}

struct Chunk<'t> {
    text: &'t [u8],
    tables: Tables<'t>,
    bases: Bases,
    line_bytes: usize,
    /// Each skipped keyword's place in `extras.skipped`.
    skipped: HashMap<&'t [u8], usize>,
    // Records written so far, local to the chunk.
    positions: usize,
    texcoords: usize,
    normals: usize,
    faces: usize,
    corners: usize,
    extras: Extras,
}

/// The 0-based element a reference names among `count` declared so far:
/// positive references count from 1, negative ones back from the last.
#[inline(always)]
fn resolve(value: Option<i64>, count: usize, at: usize) -> Result<u32, Fault> {
    let Some(value) = value else {
        return Err(Fault::new(MalformedKind::Syntax, at, "expected an index"));
    };
    let index = if value > 0 {
        value - 1
    } else {
        count as i64 + value
    };
    if value == 0 || index < 0 || index as usize >= count {
        return Err(Fault::new(
            MalformedKind::Index,
            at,
            format!("reference {value} is out of range ({count} declared before it)"),
        ));
    }
    Ok(index as u32)
}

impl<'t> Chunk<'t> {
    fn lines(&mut self, start: usize, end: usize) -> Result<(), Fault> {
        let bytes = self.text;
        let mut at = start;
        while at < end {
            let stop =
                kinogaki_bytes::find(b'\n', &bytes[at..end]).map_or(end, |offset| at + offset);
            if stop - at > self.line_bytes {
                return Err(Fault::new(
                    MalformedKind::Limit,
                    at,
                    "line exceeds the length limit",
                ));
            }
            let mut line = Line {
                bytes,
                at,
                end: stop,
            };
            if let Some(keyword) = line.token() {
                continued(bytes, at, stop)?;
                self.record(keyword, &mut line)?;
            }
            at = stop + 1;
        }
        Ok(())
    }

    fn record(&mut self, keyword: &'t [u8], line: &mut Line<'t>) -> Result<(), Fault> {
        match keyword {
            b"v" => self.position(line),
            b"vt" => {
                let u = line.number()?;
                let v = line.optional(0.0)?;
                let w = line.optional(0.0)?;
                line.finish("vt takes at most three values")?;
                self.tables.texcoords[self.texcoords] = [u, v, w];
                self.texcoords += 1;
                Ok(())
            }
            b"vn" => {
                let normal = [line.number()?, line.number()?, line.number()?];
                line.finish("vn takes three values")?;
                self.tables.normals[self.normals] = normal;
                self.normals += 1;
                Ok(())
            }
            b"f" => self.face(line),
            b"l" => self.element(line, false),
            b"p" => self.element(line, true),
            b"o" => {
                let value = line.rest();
                let run = self.run(value);
                self.extras.objects.push(run);
                Ok(())
            }
            b"g" => {
                let value = line.rest().split_whitespace().collect::<Vec<_>>().join(" ");
                let run = self.run(value);
                self.extras.groups.push(run);
                Ok(())
            }
            b"usemtl" => {
                let value = line.rest();
                let run = self.run(value);
                self.extras.materials.push(run);
                Ok(())
            }
            b"s" => self.smoothing(line),
            b"mtllib" => {
                let value = line.rest();
                self.extras.libraries.push(value);
                Ok(())
            }
            _ => {
                // Free-form geometry (`vp`, `curv`, `surf`...), display and
                // render statements and unknown keywords alike are skipped and
                // reported, never fatal.
                let start = line.at - keyword.len();
                match self.skipped.get(keyword) {
                    Some(&entry) => self.extras.skipped[entry].1 += 1,
                    None => {
                        self.skipped.insert(keyword, self.extras.skipped.len());
                        self.extras.skipped.push((keyword.to_vec(), 1, start));
                    }
                }
                Ok(())
            }
        }
    }

    /// `v x y z`, `v x y z w`, `v x y z r g b` or `v x y z w r g b`.
    fn position(&mut self, line: &mut Line<'t>) -> Result<(), Fault> {
        const SHAPE: &str = "expected v x y z [w] [r g b]";
        let position = [line.number()?, line.number()?, line.number()?];
        let mut extra = [0.0; 4];
        let mut count = 0;
        let at = line.at;
        while line.more() {
            if count == 4 {
                return Err(Fault::new(MalformedKind::Syntax, line.at, SHAPE));
            }
            extra[count] = line.number()?;
            count += 1;
        }
        let index = self.positions as u32;
        let weight = match count {
            0 => None,
            1 => Some(extra[0]),
            3 => {
                self.extras
                    .colors
                    .push((index, [extra[0], extra[1], extra[2]]));
                None
            }
            4 => {
                self.extras
                    .colors
                    .push((index, [extra[1], extra[2], extra[3]]));
                Some(extra[0])
            }
            _ => return Err(Fault::new(MalformedKind::Syntax, line.at, SHAPE)),
        };
        if let Some(weight) = weight {
            if weight == 0.0 {
                return Err(Fault::new(
                    MalformedKind::Number,
                    at,
                    "a vertex weight cannot be 0",
                ));
            }
            self.extras.weights.push((index, weight));
        }
        self.tables.positions[index as usize] = position;
        self.positions += 1;
        Ok(())
    }

    /// `f p[/t][/n] ...` with at least three corners.
    fn face(&mut self, line: &mut Line<'t>) -> Result<(), Fault> {
        let bytes = line.bytes;
        let end = line.end;
        let positions = self.bases.positions + self.positions;
        let texcoords = self.bases.texcoords + self.texcoords;
        let normals = self.bases.normals + self.normals;
        let keep_texcoords = !self.tables.corner_texcoords.is_empty();
        let keep_normals = !self.tables.corner_normals.is_empty();
        let first = self.corners;
        // Errors about the whole record point at its one-letter keyword.
        let start = line.at - 1;
        while line.more() {
            let at = line.at;
            let (value, stop) = parse_index(bytes, at, end, true);
            let position = resolve(value, positions, at)?;
            let mut texcoord = NONE;
            let mut normal = NONE;
            let mut cursor = stop;
            if cursor < end && bytes[cursor] == b'/' {
                cursor += 1;
                if cursor < end && bytes[cursor] != b'/' && !delimiter(bytes[cursor]) {
                    let (value, stop) = parse_index(bytes, cursor, end, true);
                    texcoord = resolve(value, texcoords, cursor)?;
                    cursor = stop;
                    self.extras.face_texcoords = true;
                }
                if cursor < end && bytes[cursor] == b'/' {
                    cursor += 1;
                    let (value, stop) = parse_index(bytes, cursor, end, false);
                    normal = resolve(value, normals, cursor)?;
                    cursor = stop;
                    self.extras.face_normals = true;
                }
            }
            if cursor < end && !delimiter(bytes[cursor]) {
                return Err(Fault::new(
                    MalformedKind::Syntax,
                    cursor,
                    "unexpected text in a face corner",
                ));
            }
            line.at = cursor;
            let corner = self.corners;
            self.tables.corner_positions[corner] = position;
            if keep_texcoords {
                self.tables.corner_texcoords[corner] = texcoord;
            }
            if keep_normals {
                self.tables.corner_normals[corner] = normal;
            }
            self.corners += 1;
        }
        if self.corners - first < 3 {
            return Err(Fault::new(
                MalformedKind::Syntax,
                start,
                "a face needs at least three corners",
            ));
        }
        self.tables.face_ends[self.faces] = (self.bases.corners + self.corners) as u32;
        self.faces += 1;
        Ok(())
    }

    /// `l p[/t] ...` (at least two corners) or `p p ...` (at least one).
    fn element(&mut self, line: &mut Line<'t>, point: bool) -> Result<(), Fault> {
        let bytes = line.bytes;
        let end = line.end;
        let positions = self.bases.positions + self.positions;
        let texcoords = self.bases.texcoords + self.texcoords;
        let elements = if point {
            &mut self.extras.points
        } else {
            &mut self.extras.lines
        };
        let first = elements.positions.len();
        // Errors about the whole record point at its one-letter keyword.
        let start = line.at - 1;
        while line.more() {
            let at = line.at;
            let (value, stop) = parse_index(bytes, at, end, !point);
            elements.positions.push(resolve(value, positions, at)?);
            let mut cursor = stop;
            if !point {
                let mut texcoord = NONE;
                if cursor < end && bytes[cursor] == b'/' {
                    cursor += 1;
                    let (value, stop) = parse_index(bytes, cursor, end, false);
                    texcoord = resolve(value, texcoords, cursor)?;
                    cursor = stop;
                    self.extras.line_texcoords = true;
                }
                elements.texcoords.push(texcoord);
            }
            if cursor < end && !delimiter(bytes[cursor]) {
                return Err(Fault::new(
                    MalformedKind::Syntax,
                    cursor,
                    "unexpected text in a reference",
                ));
            }
            line.at = cursor;
        }
        let count = elements.positions.len() - first;
        if count < if point { 1 } else { 2 } {
            let message = if point {
                "a point record needs a reference"
            } else {
                "a line needs at least two corners"
            };
            return Err(Fault::new(MalformedKind::Syntax, start, message));
        }
        if elements.positions.len() > CORNERS {
            return Err(Fault::new(MalformedKind::Limit, start, "too many corners"));
        }
        elements.offsets.push(elements.positions.len() as u32);
        Ok(())
    }

    /// `s n` or `s off`.
    fn smoothing(&mut self, line: &mut Line<'t>) -> Result<(), Fault> {
        line.skip();
        let at = line.at;
        let value = match line.token() {
            Some(b"off") => 0,
            Some(token) => std::str::from_utf8(token)
                .ok()
                .and_then(|text| text.parse::<u32>().ok())
                .ok_or_else(|| {
                    Fault::new(
                        MalformedKind::Syntax,
                        at,
                        "expected a smoothing group number or off",
                    )
                })?,
            None => {
                return Err(Fault::new(
                    MalformedKind::Syntax,
                    at,
                    "expected a smoothing group",
                ));
            }
        };
        line.finish("s takes one value")?;
        let run = self.run(value);
        self.extras.smoothing.push(run);
        Ok(())
    }

    /// A run starting at the chunk's current element counts.
    fn run<T>(&self, value: T) -> Run<T> {
        Run {
            face: self.faces as u32,
            line: self.extras.lines.len() as u32,
            point: self.extras.points.len() as u32,
            value,
        }
    }
}

/// Fails on a `\` line continuation, which this reader does not join: a line
/// whose last byte (before trailing blanks) is a backslash and that has no
/// comment.
#[inline(always)]
fn continued(bytes: &[u8], start: usize, mut stop: usize) -> Result<(), Fault> {
    while stop > start && separator(bytes[stop - 1]) {
        stop -= 1;
    }
    if stop > start && bytes[stop - 1] == b'\\' && !commented(&bytes[start..stop]) {
        return Err(Fault::new(
            MalformedKind::Unsupported,
            stop - 1,
            "line continuations are not supported",
        ));
    }
    Ok(())
}

/// Whether a `#` starts a token in `line`.
#[cold]
fn commented(line: &[u8]) -> bool {
    line.iter()
        .enumerate()
        .any(|(at, &byte)| byte == b'#' && (at == 0 || separator(line[at - 1])))
}
