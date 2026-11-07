//! An OBJ file to one [`Geometry`]: polygons then polylines over its points,
//! vertex `uv` and `N`, point `Cd`, primitive `name`, `material` and groups,
//! placed from the file's units and up axis into the project's; and the
//! materials of the libraries it names (`mtllib`, beside it) as MaterialX
//! networks, keyed by the names its `usemtl` statements give.
use crate::materials::read::{Maps, materials};
use crate::obj::mtl;
use crate::obj::{Elements, NONE, Obj, Run, Skipped, run_at};
use crate::{COLORS, GEOMETRY, NORMAL_MAPS, UNITS, at_file};
use kinogaki_studio_api::edit::editor::{Placed, gather};
use kinogaki_studio_api::format::options;
use kinogaki_studio_api::geometry::{
    Attribute, Attributes, Domain, Elements as Members, Geometry, Kind, Meaning, Primitives, Text,
    Units, UpAxis, Vec3, Xform, names,
};
use kinogaki_studio_api::standard::Materials;
use kinogaki_studio_api::{
    Diagnostic, Error, FileItem, FormatOptions, Imported, Input, Reading, Value, standard,
};
use std::collections::BTreeMap;
use std::path::Path;

/// Reads `input` as `options` say: a file on disk streamed, one in memory
/// parsed whole.
pub(crate) fn read(
    input: &Input<'_>,
    chosen: &FormatOptions,
    reading: &Reading<'_>,
) -> Result<Imported, Error> {
    reading.check()?;
    let obj = match input.path() {
        Some(path) => at_file(input, crate::obj::read_file(path))?,
        None => at_file(input, crate::obj::parse(&input.bytes()?))?,
    };
    reading.check()?;
    let wants = |ty| reading.wants.is_none_or(|w| w == ty);
    let libraries = match options::option(chosen, "materials", Value::bool)? {
        true => obj.material_libraries.clone(),
        false => Vec::new(),
    };
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    if wants(*GEOMETRY) {
        let (value, left) = geometry(obj, chosen, reading)?;
        let name = Path::new(input.name())
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        items.push(FileItem::top(name, standard::geometry().wrap(value)));
        warnings.extend(left);
    }
    if wants(standard::materials().id()) && !libraries.is_empty() {
        let maps = Maps {
            folder: None,
            embed: options::option(chosen, options::EMBED, Value::bool)?,
            normal_maps: options::option(chosen, NORMAL_MAPS, Value::bool)?,
        };
        let held = libraries_of(&libraries, input, maps, &mut warnings);
        items.insert(
            0,
            FileItem::top("Materials", standard::materials().wrap(held)),
        );
    }
    Ok(Imported { items, warnings })
}

/// The materials of the libraries `libraries` name (each `mtllib` value),
/// read beside `input`, the first of a name kept; what couldn't be read
/// said in `warnings`.
fn libraries_of(
    libraries: &[String],
    input: &Input<'_>,
    maps: Maps<'_>,
    warnings: &mut Vec<Diagnostic>,
) -> Materials {
    let mut held = Materials::default();
    let Some(folder) = input.path().and_then(Path::parent) else {
        let names = libraries.join(", ");
        warnings.push(Diagnostic::warning(format!(
            "{input} is read from memory, so its material libraries ({names}) are left out"
        )));
        return held;
    };
    for value in libraries {
        let found = mtl::library_paths(value)
            .into_iter()
            .map(|name| folder.join(name))
            .find(|path| path.is_file());
        let Some(path) = found else {
            let at = folder.display();
            warnings.push(Diagnostic::warning(format!(
                "mtllib {value}: no such file in {at}"
            )));
            continue;
        };
        let library = match mtl::read_file(&path) {
            Ok(library) => library,
            Err(e) => {
                let at = path.display();
                warnings.push(Diagnostic::warning(format!(
                    "{at}: {e}; its materials are left out"
                )));
                continue;
            }
        };
        let maps = Maps {
            folder: path.parent(),
            ..maps
        };
        let (made, left) = materials(&library, maps);
        warnings.extend(left);
        for m in made {
            match held.materials.iter().any(|h| h.key == m.key) {
                true => warnings.push(Diagnostic::warning(format!(
                    "{}: a second material of that name, in {}, left out",
                    m.key,
                    path.display()
                ))),
                false => held.materials.push(m),
            }
        }
    }
    held
}

/// A file's elements kept in the table, by their numbers in the file.
struct Kept {
    faces: Elements,
    /// Each kept face's and line's number in the file.
    face_from: Vec<u32>,
    lines: Elements,
    line_from: Vec<u32>,
}

/// `obj` as a value in the project's units, with what it left out.
fn geometry(
    obj: Obj,
    chosen: &FormatOptions,
    reading: &Reading<'_>,
) -> Result<(Geometry, Vec<Diagnostic>), Error> {
    let keeps = |name| options::option(chosen, name, Value::bool);
    let (keep_normals, keep_uvs, keep_materials) =
        (keeps("normals")?, keeps("uvs")?, keeps("materials")?);
    let by_groups = options::option(chosen, "pieces", Value::choice)? == "g";
    let Obj {
        positions,
        weights,
        colors,
        texcoords,
        normals,
        faces,
        lines,
        points,
        objects,
        groups,
        materials,
        skipped,
        ..
    } = obj;
    let (dropped_faces, dropped_lines) = (faces.len(), lines.len());
    let kept = kept_elements(faces, lines);
    let dropped_faces = dropped_faces - kept.faces.len();
    let dropped_lines = dropped_lines - kept.lines.len();
    let warnings = left_out(dropped_faces, dropped_lines, points.len(), &skipped);
    let at: Vec<[f64; 3]> = match weights.is_empty() {
        true => positions,
        false => (positions.iter().zip(&weights))
            .map(|(p, w)| p.map(|c| c / w))
            .collect(),
    };
    let table = table(at.len(), &kept)?;
    let mut attributes = Attributes::new();
    if keep_uvs && (!kept.faces.texcoords.is_empty() || !kept.lines.texcoords.is_empty()) {
        let uv = corners(&kept, |e| &e.texcoords, |i| texcoords[i], [0.0; 3]);
        let uv: Vec<[f32; 2]> = uv.iter().map(|t| [t[0] as f32, t[1] as f32]).collect();
        let column = Attribute::tuples(uv, Meaning::TextureCoord)?;
        attributes = attributes.with(Domain::Vertex, names::UV, column)?;
    }
    if keep_normals && !kept.faces.normals.is_empty() {
        let n = corners(&kept, |e| &e.normals, |i| normals[i], [0.0; 3]);
        let n: Vec<[f32; 3]> = n.iter().map(|v| v.map(|c| c as f32)).collect();
        let column = Attribute::tuples(n, Meaning::Normal)?;
        attributes = attributes.with(Domain::Vertex, names::N, column)?;
    }
    if !colors.is_empty() {
        let mut rgb = colors;
        reading.colors.to_working(COLORS, &mut rgb)?;
        let cd: Vec<[f32; 3]> = rgb.iter().map(|c| c.map(|v| v as f32)).collect();
        let column = Attribute::tuples(cd, Meaning::Color)?;
        attributes = attributes.with(Domain::Point, names::CD, column)?;
    }
    if keep_materials && !materials.is_empty() {
        let column = text_column(&kept, &materials);
        attributes = attributes.with(Domain::Primitive, names::MATERIAL, column)?;
    }
    let pieces = if by_groups { &groups } else { &objects };
    if !pieces.is_empty() {
        let column = text_column(&kept, pieces);
        attributes = attributes.with(Domain::Primitive, names::NAME, column)?;
    }
    let primitives = kept.faces.len() + kept.lines.len();
    for (name, members) in primitive_groups(&kept, &groups) {
        let members = Members::from_indices(primitives, members)?;
        attributes = attributes.with_group(Domain::Primitive, &name, members)?;
    }
    let value = Geometry::new(at.into(), table, attributes)?;
    let at = placement(chosen, reading.units)?;
    Ok((gather(&[Placed { value: &value, at }])?, warnings))
}

/// File points to the project's: the file's unit over the project's, the
/// scale, and the turn between their up axes.
pub(crate) fn placement(chosen: &FormatOptions, target: Units) -> Result<Xform, Error> {
    let key = options::option(chosen, "unit", Value::choice)?;
    let (_, _, meters) = (UNITS.iter())
        .find(|u| u.0 == key)
        .ok_or_else(|| Error::invalid(format!("no unit {key}")))?;
    let scale = options::option(chosen, "scale", Value::number)?;
    let k = meters / target.meters_per_unit() * scale;
    let from = match options::option(chosen, "up", Value::choice)? {
        "z" => UpAxis::Z,
        _ => UpAxis::Y,
    };
    let (x, y, z) = match (from, target.up()) {
        (UpAxis::Y, UpAxis::Z) => (Vec3::X, Vec3::Z, Vec3::Y * -1.0),
        (UpAxis::Z, UpAxis::Y) => (Vec3::X, Vec3::Y * -1.0, Vec3::Z),
        _ => (Vec3::X, Vec3::Y, Vec3::Z),
    };
    Ok(Xform::from_axes(
        x * k,
        y * k,
        z * k,
        Vec3::new(0.0, 0.0, 0.0),
    ))
}

/// The polygons naming no point twice and the polylines, consecutive repeats
/// of a point merged, of at least two points.
fn kept_elements(faces: Elements, lines: Elements) -> Kept {
    let mut kept = Kept {
        faces: Elements::default(),
        face_from: Vec::with_capacity(faces.len()),
        lines: Elements::default(),
        line_from: Vec::with_capacity(lines.len()),
    };
    for face in 0..faces.len() {
        let corners = faces.corners(face);
        let repeats = (1..corners.len()).any(|i| corners[..i].contains(&corners[i]));
        if !repeats {
            push(&mut kept.faces, &faces, faces.range(face));
            kept.face_from.push(face as u32);
        }
    }
    for line in 0..lines.len() {
        let range = lines.range(line);
        let start = kept.lines.positions.len();
        for corner in range {
            let p = lines.positions[corner];
            if kept.lines.positions.len() == start || kept.lines.positions.last() != Some(&p) {
                push_corner(&mut kept.lines, &lines, corner);
            }
        }
        if kept.lines.positions.len() - start < 2 {
            kept.lines.positions.truncate(start);
            kept.lines
                .texcoords
                .truncate(start.min(kept.lines.texcoords.len()));
            continue;
        }
        kept.lines.offsets.push(kept.lines.positions.len() as u32);
        kept.line_from.push(line as u32);
    }
    kept
}

fn push(to: &mut Elements, from: &Elements, range: std::ops::Range<usize>) {
    range.for_each(|corner| push_corner(to, from, corner));
    to.offsets.push(to.positions.len() as u32);
}

fn push_corner(to: &mut Elements, from: &Elements, corner: usize) {
    to.positions.push(from.positions[corner]);
    if !from.texcoords.is_empty() {
        to.texcoords.push(from.texcoords[corner]);
    }
    if !from.normals.is_empty() {
        to.normals.push(from.normals[corner]);
    }
}

/// The table: polygons, then polylines.
fn table(points: usize, kept: &Kept) -> Result<Primitives, Error> {
    let at = kept.faces.positions.len() as u32;
    let mut offsets = kept.faces.offsets.clone();
    offsets.extend(kept.lines.offsets[1..].iter().map(|&o| o + at));
    let mut corners = kept.faces.positions.clone();
    corners.extend_from_slice(&kept.lines.positions);
    let mut kinds = vec![Kind::POLYGON; kept.faces.len()];
    kinds.resize(kept.faces.len() + kept.lines.len(), Kind::POLYLINE);
    Primitives::new(points, offsets.into(), corners.into(), kinds.into(), None)
}

/// One value a kept corner, polygons then polylines: its index in the file's
/// table, or `missing` where it names none.
fn corners<T: Copy>(
    kept: &Kept,
    column: impl Fn(&Elements) -> &Vec<u32>,
    read: impl Fn(usize) -> T,
    missing: T,
) -> Vec<T> {
    let mut out = Vec::with_capacity(kept.faces.positions.len() + kept.lines.positions.len());
    for elements in [&kept.faces, &kept.lines] {
        let own = column(elements);
        if own.is_empty() {
            out.extend(std::iter::repeat_n(missing, elements.positions.len()));
        } else {
            out.extend(own.iter().map(|&i| match i {
                NONE => missing,
                i => read(i as usize),
            }));
        }
    }
    out
}

/// The run of `runs` in effect for each kept primitive.
fn runs_of<'a>(kept: &'a Kept, runs: &'a [Run<String>]) -> impl Iterator<Item = &'a str> + 'a {
    let faces = (kept.face_from.iter()).map(|&f| run_at(runs, f, |r| r.face));
    let lines = (kept.line_from.iter()).map(|&l| run_at(runs, l, |r| r.line));
    faces
        .chain(lines)
        .map(|run| run.map_or("", |r| r.value.as_str()))
}

fn text_column(kept: &Kept, runs: &[Run<String>]) -> Attribute {
    Attribute::text(Text::from_strs(runs_of(kept, runs)))
}

/// Members of every `g` name, leaving out what a group can't be called (a
/// hidden `.name`, too long) and the names attributes use.
fn primitive_groups(kept: &Kept, runs: &[Run<String>]) -> BTreeMap<String, Vec<u32>> {
    let mut groups: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (primitive, value) in runs_of(kept, runs).enumerate() {
        for name in value.split(' ') {
            let usable = names::check_name(name).is_ok() && !name.starts_with('.');
            if usable {
                groups
                    .entry(name.to_owned())
                    .or_default()
                    .push(primitive as u32);
            }
        }
    }
    groups
}

/// One warning for what the table leaves out, or none.
fn left_out(faces: usize, lines: usize, points: usize, skipped: &[Skipped]) -> Vec<Diagnostic> {
    let mut parts = Vec::new();
    if faces > 0 {
        parts.push(format!("{faces} polygons repeating a point"));
    }
    if lines > 0 {
        parts.push(format!("{lines} polylines on one point"));
    }
    if points > 0 {
        parts.push(format!("{points} point records"));
    }
    parts.extend(
        skipped
            .iter()
            .map(|s| format!("{} `{}` statements", s.count, s.keyword)),
    );
    match parts.is_empty() {
        true => Vec::new(),
        false => vec![Diagnostic::warning(format!(
            "Not imported: {}",
            parts.join(", ")
        ))],
    }
}
