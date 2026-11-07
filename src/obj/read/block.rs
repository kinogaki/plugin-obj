//! A block is a run of whole lines read and parsed at once: the whole input
//! for [`crate::obj::parse`], or one window of a stream. Each block is
//!
//! 1. split into chunks at line feeds;
//! 2. counted in parallel, on the SDK's workers ([`super::count`]);
//! 3. given its final table sizes, so every chunk gets disjoint slices;
//! 4. parsed in parallel ([`super::chunk`]), each chunk resolving references
//!    against the elements declared before each of its lines, as a sequential
//!    reader would;
//! 5. merged: the rarer records each chunk collected are appended in order.
//!
//! The first fault in file order wins, so errors do not depend on threading.
use super::{
    chunk::{self, Bases, Extras, Job, Tables},
    count::{self, Counts},
};
use crate::obj::{
    Elements, NONE, Obj, ObjError, ReadOptions, Run, Skipped, error::limit, options::CORNERS,
};
use kinogaki_studio_api::geometry::parallel;
use std::collections::HashMap;
use std::sync::Mutex;

/// The most chunks a block is split into: jobs enough for any machine's
/// workers, each still far larger than what scheduling it costs.
const MOST_CHUNKS: usize = 256;

/// An [`Obj`] under construction, fed one block at a time.
pub struct Builder {
    obj: Obj,
    options: ReadOptions,
    /// Line feeds in the blocks read so far.
    newlines: u64,
    /// Each skipped keyword's place in `obj.skipped`.
    skipped: HashMap<String, usize>,
    face_texcoords: bool,
    face_normals: bool,
    line_texcoords: bool,
}

impl Builder {
    pub fn new(options: ReadOptions) -> Self {
        Self {
            obj: Obj::default(),
            options,
            newlines: 0,
            skipped: HashMap::new(),
            face_texcoords: false,
            face_normals: false,
            line_texcoords: false,
        }
    }

    /// Parse `text`, which must end at a line feed unless it is the last block.
    pub fn block(&mut self, text: &[u8]) -> Result<(), ObjError> {
        self.options.budget.check()?;
        let ranges = split(text, self.options.chunk_bytes);
        let counts = parallel::map(ranges.len(), |at| {
            let (start, end) = ranges[at];
            count::count(&text[start..end])
        });
        self.options.budget.check()?;
        let mut total = Counts::default();
        counts.iter().for_each(|c| total.add(c));
        self.charge(&total)?;
        let first_line = self.newlines + 1;
        let obj = &mut self.obj;
        let old = Bases {
            positions: obj.positions.len(),
            texcoords: obj.texcoords.len(),
            normals: obj.normals.len(),
            corners: obj.faces.positions.len(),
        };
        let old_faces = obj.faces.len();
        obj.positions
            .resize(old.positions + total.positions, [0.0; 3]);
        obj.texcoords
            .resize(old.texcoords + total.texcoords, [0.0; 3]);
        obj.normals.resize(old.normals + total.normals, [0.0; 3]);
        let faces = &mut obj.faces;
        faces.offsets.resize(old_faces + 1 + total.faces, 0);
        faces.positions.resize(old.corners + total.corners, 0);
        // Corner vt/vn tables exist once the file has any vt/vn; earlier
        // faces could not reference one.
        let corners = old.corners + total.corners;
        if !obj.texcoords.is_empty() {
            faces.texcoords.resize(corners, NONE);
        }
        if !obj.normals.is_empty() {
            faces.normals.resize(corners, NONE);
        }

        // Disjoint slices per chunk, then the parallel parse.
        let mut positions = &mut obj.positions[old.positions..];
        let mut texcoords = &mut obj.texcoords[old.texcoords..];
        let mut normals = &mut obj.normals[old.normals..];
        let mut face_ends = &mut faces.offsets[old_faces + 1..];
        let mut corner_positions = &mut faces.positions[old.corners..];
        let mut corner_texcoords = faces.texcoords.get_mut(old.corners..).unwrap_or_default();
        let mut corner_normals = faces.normals.get_mut(old.corners..).unwrap_or_default();
        let mut bases = old;
        let mut face_bases = Vec::with_capacity(ranges.len());
        let mut jobs = Vec::with_capacity(ranges.len());
        let mut faces_before = old_faces;
        for (&(start, end), c) in ranges.iter().zip(&counts) {
            let keep_t = if corner_texcoords.is_empty() {
                0
            } else {
                c.corners
            };
            let keep_n = if corner_normals.is_empty() {
                0
            } else {
                c.corners
            };
            jobs.push(Job {
                text,
                start,
                end,
                tables: Tables {
                    positions: take(&mut positions, c.positions),
                    texcoords: take(&mut texcoords, c.texcoords),
                    normals: take(&mut normals, c.normals),
                    face_ends: take(&mut face_ends, c.faces),
                    corner_positions: take(&mut corner_positions, c.corners),
                    corner_texcoords: take(&mut corner_texcoords, keep_t),
                    corner_normals: take(&mut corner_normals, keep_n),
                },
                bases,
                line_bytes: self.options.line_bytes(),
            });
            face_bases.push(faces_before);
            bases.positions += c.positions;
            bases.texcoords += c.texcoords;
            bases.normals += c.normals;
            bases.corners += c.corners;
            faces_before += c.faces;
        }
        // Each job is taken once, by the worker given its index; a cancelled
        // read parses no more chunks.
        let jobs: Vec<Mutex<Option<Job>>> = jobs.into_iter().map(|j| Mutex::new(Some(j))).collect();
        let budget = &self.options.budget;
        let extras = parallel::map(jobs.len(), |at| {
            let job = jobs[at].lock().unwrap_or_else(|e| e.into_inner()).take();
            match (budget.cancel().is_cancelled(), job) {
                (false, Some(job)) => chunk::parse(job),
                _ => Extras::default(),
            }
        });
        self.options.budget.check()?;
        if let Some(fault) = extras.iter().find_map(|e| e.fault.clone()) {
            return Err(fault.locate(text, first_line).into());
        }

        let mut position_base = old.positions;
        let mut first_seen = Vec::new();
        for ((e, c), faces_before) in extras.into_iter().zip(&counts).zip(face_bases) {
            self.merge(e, position_base, faces_before, &mut first_seen)?;
            position_base += c.positions;
        }
        // Each keyword skipped for the first time, given its line: one
        // pass over the block for all of them.
        first_seen.sort_unstable();
        let (mut line, mut from) = (first_line, 0);
        for (at, skipped) in first_seen {
            line += kinogaki_bytes::count(b'\n', &text[from..at]) as u64;
            from = at;
            self.obj.skipped[skipped].first_line = line;
        }
        let positions = self.obj.positions.len();
        if !self.obj.weights.is_empty() {
            self.obj.weights.resize(positions, 1.0);
        }
        if !self.obj.colors.is_empty() {
            self.obj.colors.resize(positions, [1.0; 3]);
        }
        self.newlines += total.newlines;
        Ok(())
    }

    /// Charges the tables `total` counts to the budget before they are
    /// made, and refuses more corners than 32-bit offsets reach.
    fn charge(&self, total: &Counts) -> Result<(), ObjError> {
        let obj = &self.obj;
        let corners = obj.faces.positions.len()
            + obj.lines.positions.len()
            + obj.points.positions.len()
            + total.corners;
        if corners > CORNERS {
            return Err(limit("too many corners").into());
        }
        // Positions, vt and vn are three f64s; a face its end; a corner
        // its position, vt and vn.
        let vertices = (total.positions + total.texcoords + total.normals) as u64;
        let bytes = (vertices.saturating_mul(24))
            .saturating_add((total.faces as u64).saturating_mul(4))
            .saturating_add((total.corners as u64).saturating_mul(12));
        Ok(self.options.budget.allocate(bytes)?)
    }

    /// Append one chunk's collected records; each keyword skipped for the
    /// first time is added to `first_seen` by its byte offset, for its line.
    fn merge(
        &mut self,
        e: Extras,
        position_base: usize,
        faces_before: usize,
        first_seen: &mut Vec<(usize, usize)>,
    ) -> Result<(), ObjError> {
        let obj = &mut self.obj;
        let positions = obj.positions.len();
        if !e.weights.is_empty() {
            obj.weights.resize(positions, 1.0);
            for (index, weight) in e.weights {
                obj.weights[position_base + index as usize] = weight;
            }
        }
        if !e.colors.is_empty() {
            obj.colors.resize(positions, [1.0; 3]);
            for (index, color) in e.colors {
                obj.colors[position_base + index as usize] = color;
            }
        }
        let (lines, points) = (obj.lines.len() as u32, obj.points.len() as u32);
        let shift = |runs: Vec<Run<_>>, out: &mut Vec<Run<_>>| {
            out.extend(runs.into_iter().map(|run| Run {
                face: run.face + faces_before as u32,
                line: run.line + lines,
                point: run.point + points,
                value: run.value,
            }));
        };
        shift(e.objects, &mut obj.objects);
        shift(e.groups, &mut obj.groups);
        shift(e.materials, &mut obj.materials);
        let smoothing = e.smoothing.into_iter().map(|run| Run {
            face: run.face + faces_before as u32,
            line: run.line + lines,
            point: run.point + points,
            value: run.value,
        });
        obj.smoothing.extend(smoothing);
        append(&mut obj.lines, e.lines);
        append(&mut obj.points, e.points);
        obj.material_libraries.extend(e.libraries);
        for (keyword, count, at) in e.skipped {
            let keyword = String::from_utf8_lossy(&keyword).into_owned();
            match self.skipped.get(&keyword) {
                Some(&entry) => obj.skipped[entry].count += count,
                None => {
                    self.skipped.insert(keyword.clone(), obj.skipped.len());
                    first_seen.push((at, obj.skipped.len()));
                    obj.skipped.push(Skipped {
                        keyword,
                        count,
                        first_line: 0,
                    });
                }
            }
        }
        self.face_texcoords |= e.face_texcoords;
        self.face_normals |= e.face_normals;
        self.line_texcoords |= e.line_texcoords;
        let obj = &self.obj;
        let corners =
            obj.faces.positions.len() + obj.lines.positions.len() + obj.points.positions.len();
        if corners > CORNERS {
            return Err(limit("too many corners").into());
        }
        Ok(())
    }

    /// The finished file: corner vt/vn tables nobody referenced are dropped.
    pub fn finish(mut self) -> Obj {
        if !self.face_texcoords {
            self.obj.faces.texcoords = Vec::new();
        }
        if !self.face_normals {
            self.obj.faces.normals = Vec::new();
        }
        if !self.line_texcoords {
            self.obj.lines.texcoords = Vec::new();
        }
        self.obj
    }
}

/// The first `count` elements of `*slice`, leaving the rest in it.
fn take<'a, T>(slice: &mut &'a mut [T], count: usize) -> &'a mut [T] {
    let (head, tail) = std::mem::take(slice).split_at_mut(count);
    *slice = tail;
    head
}

/// Append `more` (offsets starting at 0) to `elements`.
fn append(elements: &mut Elements, more: Elements) {
    if more.is_empty() {
        return;
    }
    let base = elements.positions.len() as u32;
    // Line corners carry a vt slot per corner while building (NONE when
    // absent); point sets have none.
    if !more.texcoords.is_empty() {
        elements.texcoords.resize(elements.positions.len(), NONE);
        elements.texcoords.extend(more.texcoords);
    }
    elements
        .offsets
        .extend(more.offsets[1..].iter().map(|&end| end + base));
    elements.positions.extend(more.positions);
}

/// Chunk ranges of `text`: about `least` bytes or more each, at most
/// [`MOST_CHUNKS`], each ending just after a line feed (the last at the end
/// of the text).
pub fn split(text: &[u8], least: usize) -> Vec<(usize, usize)> {
    let wanted = (text.len() / least.max(1)).clamp(1, MOST_CHUNKS);
    let mut ranges = Vec::with_capacity(wanted);
    let mut start = 0;
    for at in 1..=wanted {
        let mut end = if at == wanted {
            text.len()
        } else {
            (text.len() / wanted * at).max(start)
        };
        if end < text.len() {
            end = kinogaki_bytes::find(b'\n', &text[end..])
                .map_or(text.len(), |offset| end + offset + 1);
        }
        if end > start {
            ranges.push((start, end));
            start = end;
        }
        if end == text.len() {
            break;
        }
    }
    if ranges.is_empty() {
        ranges.push((0, text.len()));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn chunks_cover_the_text_and_end_at_line_feeds() {
        let text: Vec<u8> = (0..200_000)
            .flat_map(|i| format!("v {i} 0 0\n").into_bytes())
            .collect();
        for least in [1 << 10, 64 << 10, 256 << 10] {
            let ranges = split(&text, least);
            assert!(ranges.len() <= super::MOST_CHUNKS);
            assert_eq!(ranges.first().unwrap().0, 0);
            assert_eq!(ranges.last().unwrap().1, text.len());
            for pair in ranges.windows(2) {
                assert_eq!(pair[0].1, pair[1].0);
                assert_eq!(text[pair[0].1 - 1], b'\n');
            }
        }
        assert_eq!(split(b"", 1), vec![(0, 0)]);
        assert_eq!(split(b"v 1 2 3", 1), vec![(0, 7)]);
        assert_eq!(split(b"a\nb\nc\n", 1), vec![(0, 2), (2, 4), (4, 6)]);
    }
}
