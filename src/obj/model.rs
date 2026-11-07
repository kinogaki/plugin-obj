//! The plain data a Wavefront OBJ file holds, independent of any mesh kernel.
//!
//! Vertex data lives in flat tables (`v`, `vt`, `vn`); faces, polylines and
//! point sets are [`Elements`]: compressed rows of corners whose indices are
//! 0-based into those tables. Statements that change the current state for the
//! elements after them (`o`, `g`, `usemtl`, `s`) become [`Run`]s that record
//! where in the element sequence they took effect.

/// No `vt` or `vn` reference at this corner.
pub const NONE: u32 = u32::MAX;

/// Everything read from one OBJ file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obj {
    /// `v x y z`, as written (not divided by any weight).
    pub positions: Vec<[f64; 3]>,
    /// The optional fourth `v` value; empty when no vertex has one, else one
    /// per position with 1 where a vertex left it out.
    pub weights: Vec<f64>,
    /// The common `v x y z [w] r g b` extension; empty when no vertex has one,
    /// else one per position with white where a vertex left it out.
    pub colors: Vec<[f64; 3]>,
    /// `vt u [v [w]]`, with missing components 0.
    pub texcoords: Vec<[f64; 3]>,
    /// `vn x y z`, as written (not normalized).
    pub normals: Vec<[f64; 3]>,
    /// `f` records: polygons of at least three corners.
    pub faces: Elements,
    /// `l` records: polylines of at least two corners (`normals` stays empty).
    pub lines: Elements,
    /// `p` records: point sets of at least one corner (only `positions`).
    pub points: Elements,
    /// `o name` statements.
    pub objects: Vec<Run<String>>,
    /// `g name...` statements; the value is the names separated by single spaces.
    pub groups: Vec<Run<String>>,
    /// `usemtl name` statements.
    pub materials: Vec<Run<String>>,
    /// `s n` statements; `s off` is 0.
    pub smoothing: Vec<Run<u32>>,
    /// The rest of each `mtllib` line, verbatim (see [`crate::obj::mtl::library_paths`]).
    pub material_libraries: Vec<String>,
    /// Statements that were recognised as OBJ but are not represented
    /// (curves, surfaces, display attributes, unknown keywords), by keyword.
    pub skipped: Vec<Skipped>,
}

/// Rows of corners: element `e` owns corners `offsets[e]..offsets[e + 1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Elements {
    /// One more than the element count; starts at 0.
    pub offsets: Vec<u32>,
    /// 0-based position index per corner.
    pub positions: Vec<u32>,
    /// 0-based `vt` index per corner, or [`NONE`]; empty when no corner has one.
    pub texcoords: Vec<u32>,
    /// 0-based `vn` index per corner, or [`NONE`]; empty when no corner has one.
    pub normals: Vec<u32>,
}

impl Default for Elements {
    fn default() -> Self {
        Self {
            offsets: vec![0],
            positions: Vec::new(),
            texcoords: Vec::new(),
            normals: Vec::new(),
        }
    }
}

impl Elements {
    /// Number of elements.
    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of corners over all elements.
    pub fn corner_count(&self) -> usize {
        self.positions.len()
    }

    /// The corner range of element `element`.
    pub fn range(&self, element: usize) -> std::ops::Range<usize> {
        self.offsets[element] as usize..self.offsets[element + 1] as usize
    }

    /// The position indices of element `element`.
    pub fn corners(&self, element: usize) -> &[u32] {
        &self.positions[self.range(element)]
    }
}

/// A state statement that applies to every element after it, until the next
/// statement of the same kind. `face`, `line` and `point` are the counts of
/// each element kind read before the statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Run<T> {
    pub face: u32,
    pub line: u32,
    pub point: u32,
    pub value: T,
}

/// A statement kind the reader skipped, how often, and where first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub keyword: String,
    pub count: u64,
    pub first_line: u64,
}

/// The run in effect for element `index`, where `start` gives a run's first
/// element of the kind in question: the last run starting at or before it.
pub fn run_at<T>(runs: &[Run<T>], index: u32, start: impl Fn(&Run<T>) -> u32) -> Option<&Run<T>> {
    let after = runs.partition_point(|run| start(run) <= index);
    after.checked_sub(1).map(|at| &runs[at])
}

impl Obj {
    /// The value of the `o`/`g`/`usemtl`/`s` run in effect for face `face`.
    pub fn face_run<T>(runs: &[Run<T>], face: u32) -> Option<&T> {
        run_at(runs, face, |run| run.face).map(|run| &run.value)
    }

    /// Position `index` divided by its weight, when the file gave one.
    pub fn position(&self, index: usize) -> [f64; 3] {
        let [x, y, z] = self.positions[index];
        match self.weights.get(index) {
            Some(&w) if w != 1.0 => [x / w, y / w, z / w],
            _ => [x, y, z],
        }
    }

    /// Whether the file holds no geometry records at all.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
            && self.texcoords.is_empty()
            && self.normals.is_empty()
            && self.faces.is_empty()
            && self.lines.is_empty()
            && self.points.is_empty()
    }
}
