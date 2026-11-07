//! Items to one OBJ file: each item's polygons and polylines at its placement,
//! Y up in metres as a read takes it by default (the comment says so, OBJ records none), `uv` and
//! `N` as `vt` and `vn`, point `Cd` as vertex colors, primitive `name` as `o`
//! (an item without one is named after the item), `material` as `usemtl` and
//! primitive groups as `g`. The materials item's Materials go to an MTL
//! library beside it (`<stem>.mtl`, named by `mtllib`), their textures named
//! relative to it, embedded ones written beside it; `usemtl` names each by
//! its name in the library.
use crate::at_file;
use crate::materials::write::library;
use crate::obj::{NONE, Obj, Run, WriteOptions};
use kinogaki_studio_api::color::SRGB_DISPLAY;
use kinogaki_studio_api::edit::editor::{Placed, gather};
use kinogaki_studio_api::format::files::Beside;
use kinogaki_studio_api::geometry::{Attribute, Domain, Geometry, Kind, Text, Tuple, names};
use kinogaki_studio_api::{
    DataTypeId, Diagnostic, Error, FileFormat, FileItem, FormatOptions, FormatWriter, Writing,
    standard,
};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) struct Writer;

/// What it writes: geometry, what the host makes polygons of, and materials.
const WRITES: &[DataTypeId] = &[
    standard::geometry().id(),
    standard::brep().id(),
    standard::instances().id(),
    standard::materials().id(),
];

impl FormatWriter for Writer {
    fn accepts(&self) -> &[DataTypeId] {
        WRITES
    }
    fn write(
        &self,
        path: &Path,
        items: &[FileItem],
        writing: &Writing<'_>,
        _: &FormatOptions,
    ) -> Result<Vec<Diagnostic>, Error> {
        let mut warnings = Vec::new();
        let mut values = Vec::new();
        let mut materials = None;
        for item in items {
            writing.check()?;
            let Some(data) = &item.data else { continue };
            if let Some(held) = standard::materials().get(data) {
                materials = Some(held);
                continue;
            }
            let value = match standard::geometry().get(data) {
                Some(value) => Some(value.clone()),
                None => (writing.meshed)(data),
            };
            match value {
                Some(value) => values.push((named(&value, &item.name)?, item.placement)),
                None => warnings.push(Diagnostic::warning(format!(
                    "{}: nothing here makes polygons of it, left out",
                    item.name
                ))),
            }
        }
        // The file holds what a read of it by default takes: Y up, in metres.
        let defaults = crate::Obj.options();
        let to_file = (crate::read::placement(&defaults, writing.units)?.inverse())
            .ok_or_else(|| Error::invalid("the project's units have no inverse"))?;
        let placed: Vec<Placed<'_>> = (values.iter())
            .map(|(value, at)| Placed {
                value,
                at: to_file.compose(at),
            })
            .collect();
        let value = gather(&placed)?;
        writing.check()?;
        let curves = (0..value.primitives().len())
            .filter(|&p| !is_written(&value, p))
            .count();
        if curves > 0 {
            warnings.push(Diagnostic::warning(format!("{curves} curves left out")));
        }
        let options = WriteOptions {
            comment: Some("Y up, in metres".to_owned()),
        };
        let mut named = BTreeMap::new();
        let mut libraries = Vec::new();
        if let Some(held) = materials.filter(|m| !m.materials.is_empty()) {
            let mtl = path.with_extension("mtl");
            let made = library(held, &mut Beside::new(&mtl), writing.colors)?;
            at_file(
                mtl.display(),
                crate::obj::mtl::write_file(&mtl, &made.library),
            )?;
            warnings.extend(made.warnings);
            named = made.names;
            let file = mtl.file_name().map(|n| n.to_string_lossy().into_owned());
            libraries.extend(file);
        }
        let mut obj = obj_of(&value, writing, &named)?;
        obj.material_libraries = libraries;
        at_file(path.display(), crate::obj::write_file(path, &obj, &options))?;
        Ok(warnings)
    }
}

/// `value` with its primitives named `name` where none has a name.
fn named(value: &Geometry, name: &str) -> Result<Geometry, Error> {
    let attributes = value.attributes();
    if attributes.get(Domain::Primitive, names::NAME).is_some() || name.is_empty() {
        return Ok(value.clone());
    }
    let each = std::iter::repeat_n(name, value.primitives().len());
    let column = Attribute::text(Text::from_strs(each));
    value.with_attributes(attributes.with(Domain::Primitive, names::NAME, column)?)
}

/// Whether primitive `p` is a polygon or a polyline: what OBJ's `f` and `l`
/// hold.
fn is_written(value: &Geometry, p: usize) -> bool {
    let kind = value.primitives().kind(p);
    kind.is_polygon() || kind == Kind::POLYLINE
}

/// `value` as the OBJ model ([`Obj`]), each material named as `library` says
/// (by its key), else as the value names it.
fn obj_of(
    value: &Geometry,
    writing: &Writing<'_>,
    library: &BTreeMap<String, String>,
) -> Result<Obj, Error> {
    let t = value.primitives();
    let mut obj = Obj {
        positions: value.positions().to_vec(),
        ..Obj::default()
    };
    let attributes = value.attributes();
    let uv = Corners::<[f32; 2]>::of(value, names::UV);
    let n = Corners::<[f32; 3]>::of(value, names::N);
    let (mut uvs, mut normals) = (Table::new(value), Table::new(value));
    let mut order = Vec::new();
    obj.faces.offsets = vec![0];
    obj.lines.offsets = vec![0];
    for p in 0..t.len() {
        let polygon = t.kind(p).is_polygon();
        if !polygon {
            continue;
        }
        order.push(p);
        for (vertex, &point) in t.primitive_vertices(p).zip(t.primitive(p)) {
            obj.faces.positions.push(point);
            if let Some(at) = uv.at(vertex, point) {
                let at = at.map(f64::from);
                obj.faces
                    .texcoords
                    .push(uvs.index(point, [at[0], at[1], 0.0]));
            }
            if let Some(at) = n.at(vertex, point) {
                obj.faces
                    .normals
                    .push(normals.index(point, at.map(f64::from)));
            }
        }
        obj.faces.offsets.push(obj.faces.positions.len() as u32);
    }
    for p in (0..t.len()).filter(|&p| is_written(value, p) && !t.kind(p).is_polygon()) {
        order.push(p);
        for (vertex, &point) in t.primitive_vertices(p).zip(t.primitive(p)) {
            obj.lines.positions.push(point);
            if let Some(at) = uv.at(vertex, point) {
                let at = at.map(f64::from);
                obj.lines
                    .texcoords
                    .push(uvs.index(point, [at[0], at[1], 0.0]));
            }
        }
        obj.lines.offsets.push(obj.lines.positions.len() as u32);
    }
    obj.texcoords = uvs.values;
    obj.normals = normals.values;
    if let Some(cd) = attributes.tuples::<[f32; 3]>(Domain::Point, names::CD) {
        let mut rgb: Vec<[f64; 3]> = cd.iter().map(|c| c.map(f64::from)).collect();
        writing.colors.working_to(SRGB_DISPLAY, &mut rgb)?;
        obj.colors = rgb;
    }
    let faces = obj.faces.len();
    if let Some(text) = attributes.text(Domain::Primitive, names::MATERIAL) {
        let named = |p: usize| {
            let key = text.get(p).unwrap_or("");
            one_line(library.get(key).map_or(key, String::as_str))
        };
        obj.materials = runs(faces, order.iter().map(|&p| named(p)));
    }
    if let Some(text) = attributes.text(Domain::Primitive, names::NAME) {
        obj.objects = runs(
            faces,
            order.iter().map(|&p| one_line(text.get(p).unwrap_or(""))),
        );
    }
    let groups: Vec<_> = (attributes.groups())
        .filter(|(domain, name, _)| *domain == Domain::Primitive && !name.starts_with('.'))
        .collect();
    if !groups.is_empty() {
        let names_of = |p: usize| {
            let held = groups.iter().filter(|g| g.2.contains(p as u32));
            held.map(|g| g.1.split_whitespace().collect::<Vec<_>>().join("_"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        obj.groups = runs(faces, order.iter().map(|&p| names_of(p)));
    }
    Ok(obj)
}

/// One attribute of a value by corner, from the Vertex domain or else the
/// Point's.
struct Corners<'a, T> {
    vertices: Option<&'a [T]>,
    points: Option<&'a [T]>,
}

impl<'a, T: Tuple + Copy> Corners<'a, T> {
    fn of(value: &'a Geometry, name: &str) -> Self {
        let a = value.attributes();
        Self {
            vertices: a.tuples(Domain::Vertex, name),
            points: a.tuples(Domain::Point, name),
        }
    }

    fn at(&self, vertex: usize, point: u32) -> Option<T> {
        (self.vertices.map(|v| v[vertex])).or_else(|| self.points.map(|v| v[point as usize]))
    }
}

/// A `vt` or `vn` table: a corner reuses its point's first record when it
/// holds the same value, so smooth data writes one record a point.
struct Table {
    values: Vec<[f64; 3]>,
    first: Vec<u32>,
}

impl Table {
    fn new(value: &Geometry) -> Self {
        Self {
            values: Vec::new(),
            first: vec![NONE; value.positions().len()],
        }
    }

    fn index(&mut self, point: u32, value: [f64; 3]) -> u32 {
        let first = self.first[point as usize];
        if first != NONE && self.values[first as usize] == value {
            return first;
        }
        let at = self.values.len() as u32;
        if first == NONE {
            self.first[point as usize] = at;
        }
        self.values.push(value);
        at
    }
}

/// A statement's value OBJ reads back as it is: one line, no comment.
fn one_line(value: &str) -> String {
    value.replace(['\n', '\r', '#'], " ").trim().to_owned()
}

/// Runs where consecutive primitives' values change: the first `faces` are
/// `f`, the rest `l`.
fn runs(faces: usize, values: impl Iterator<Item = String>) -> Vec<Run<String>> {
    let mut runs: Vec<Run<String>> = Vec::new();
    for (at, value) in values.enumerate() {
        if runs.last().is_none_or(|run| run.value != value) {
            let (face, line) = if at < faces {
                (at, 0)
            } else {
                (faces, at - faces)
            };
            runs.push(Run {
                face: face as u32,
                line: line as u32,
                point: 0,
                value,
            });
        }
    }
    runs
}
