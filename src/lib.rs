//! Wavefront OBJ, a format plugin: an OBJ file reads as one standard
//! [`Geometry`](kinogaki_studio_api::geometry::Geometry), its material
//! libraries (`mtllib`) as MaterialX networks ([`materials`]) in a materials
//! item before it; any polygons and polylines write as one OBJ, and the
//! Materials they name as an MTL library beside it. It owns the format; the
//! object a read value lands in adopts it (Mesh's, any kind's that takes
//! geometry). The files themselves are read and written by [`obj`], which
//! uses nothing of the SDK but the read's budget.
pub mod materials;
pub mod obj;
mod read;
mod write;

use kinogaki_studio_api::format::options;
use kinogaki_studio_api::params::{Choice, Parameter, Schema, ValueKind};
use kinogaki_studio_api::{
    DataTypeId, Error, FileFormat, FormatId, FormatOptions, FormatWriter, Imported, Input, Package,
    PackageId, Purpose, Reading, Value, standard,
};

/// The package's id.
pub fn id() -> PackageId {
    PackageId::declared("kinogaki.obj")
}

/// The format's id.
pub fn format() -> FormatId {
    FormatId::declared("kinogaki.format.obj")
}

/// The package: the format alone.
pub fn package() -> Package {
    Package::new(id(), "OBJ").with_format(Obj)
}

/// The lengths an OBJ's unit may stand for: its key, label and metres.
const UNITS: [(&str, &str, f64); 5] = [
    ("mm", "Millimetres", 0.001),
    ("cm", "Centimetres", 0.01),
    ("m", "Metres", 1.0),
    ("in", "Inches", 0.0254),
    ("ft", "Feet", 0.3048),
];

/// What a read makes: the geometry, and the materials its libraries hold.
const PRODUCES: &[DataTypeId] = &[standard::geometry().id(), standard::materials().id()];

/// The option reading `bump` and `map_Bump` as normal maps (D19).
const NORMAL_MAPS: &str = "normal_maps";

/// The metadata naming the MTL library a file named first.
const MTL: &str = "mtl";

/// The format.
pub struct Obj;

impl FileFormat for Obj {
    fn id(&self) -> FormatId {
        format()
    }
    fn label(&self) -> &str {
        "Wavefront OBJ"
    }
    fn short_label(&self) -> &str {
        "OBJ"
    }
    fn extensions(&self) -> &[&str] {
        &["obj"]
    }
    fn produces(&self) -> &[DataTypeId] {
        PRODUCES
    }
    /// OBJ records neither units nor an up axis: the file's are taken as
    /// these unless set (Y up in metres, what most exporters write).
    fn options(&self) -> FormatOptions {
        FormatOptions::from([
            ("up".to_owned(), Value::Choice("y".into())),
            ("unit".to_owned(), Value::Choice("m".into())),
            ("scale".to_owned(), Value::Number(1.0)),
            ("pieces".to_owned(), Value::Choice("o".into())),
            ("normals".to_owned(), Value::Bool(true)),
            ("uvs".to_owned(), Value::Bool(true)),
            ("materials".to_owned(), Value::Bool(true)),
            (options::EMBED.to_owned(), Value::Bool(false)),
            (NORMAL_MAPS.to_owned(), Value::Bool(false)),
        ])
    }
    fn schema(&self, chosen: &FormatOptions, purpose: Purpose) -> Schema {
        // An option not chosen shows its default (a File node's options at
        // their defaults name none).
        let defaults = self.options();
        let row = |name: &str, label: &str, choices: Vec<Choice>| {
            let value = (chosen.get(name).or_else(|| defaults.get(name)).cloned())
                .expect("every option has a default");
            Parameter::new(name, label, ValueKind::choice(choices), value)
        };
        let toggle = |schema, name: &str, label: &str, words: &str| {
            let on = options::option(chosen, name, Value::bool)
                .or_else(|_| options::option(&defaults, name, Value::bool))
                .unwrap_or(true);
            options::toggle(schema, name, label, words, on)
        };
        let units = UNITS.iter().map(|(k, l, _)| Choice::new(k, l)).collect();
        let up = vec![Choice::new("y", "Y"), Choice::new("z", "Z")];
        let pieces = vec![
            Choice::new("o", "Objects (o)"),
            Choice::new("g", "Groups (g)"),
        ];
        // File ▸ Open reads the file as it is: no Scale, its textures
        // where they are.
        let document = purpose == Purpose::Document;
        let schema = Schema::default().parameter(row("up", "Up Axis", up));
        let schema = schema.parameter(row("unit", "Unit", units));
        let scale = options::option(chosen, "scale", Value::number).unwrap_or(1.0);
        let schema = match document {
            true => schema,
            false => options::scale(schema, scale),
        };
        let schema = schema.parameter(row("pieces", "Pieces", pieces));
        let schema = toggle(schema, "normals", "Normals", "vn");
        let schema = toggle(schema, "uvs", "UVs", "vt texture");
        let schema = toggle(schema, "materials", "Materials", "usemtl mtllib mtl");
        if purpose == Purpose::Value {
            return schema;
        }
        let on = |name| options::option(chosen, name, Value::bool).unwrap_or(false);
        let schema = match document {
            true => schema,
            false => options::embed(schema, on(options::EMBED)),
        };
        let words = "map_Bump bump Blender normal";
        toggle(schema, NORMAL_MAPS, "Bump Maps as Normal Maps", words)
    }
    fn read(
        &self,
        input: &Input<'_>,
        options: &FormatOptions,
        reading: &Reading<'_>,
    ) -> Result<Imported, Error> {
        read::read(input, options, reading)
    }
    fn writer(&self) -> Option<&dyn FormatWriter> {
        Some(&write::Writer)
    }
}

/// `result` with the OBJ reader's or writer's error as ours, naming `file`
/// (a path, or an input).
fn at_file<T>(
    file: impl std::fmt::Display,
    result: Result<T, crate::obj::ObjError>,
) -> Result<T, Error> {
    result.map_err(|e| match e {
        crate::obj::ObjError::Cancelled => Error::cancelled(),
        e => Error::invalid(format!("{file}: {e}")),
    })
}
