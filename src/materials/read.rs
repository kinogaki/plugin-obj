//! A material library's materials as MaterialX networks (the module docs
//! say how each statement maps).
use super::{
    BUMP, DISPLACEMENT, Kind, NORMAL, PBR_ONLY, PHONG, PHONG_STATEMENTS, STANDARD,
    STANDARD_STATEMENTS, Statement, WEIGHTS, beside, sig,
};
use crate::obj::mtl::{Material, MaterialLibrary, TextureMap};
use kinogaki_studio_api::geometry::image::{EmbeddedFile, ImageFile, ImageSource};
use kinogaki_studio_api::image_files::image_node;
use kinogaki_studio_api::network::{NetworkSeed, NewNode};
use kinogaki_studio_api::standard::ImportedMaterial;
use kinogaki_studio_api::{Diagnostic, Value};
use std::path::Path;
use std::sync::Arc;

/// How a library's maps are read.
#[derive(Clone, Copy, Debug)]
pub struct Maps<'a> {
    /// The library's folder, which its maps' paths are relative to; none for
    /// a library held in memory, whose paths stay as written.
    pub folder: Option<&'a Path>,
    /// Whether the files the maps name are read in and embedded.
    pub embed: bool,
    /// Whether `bump` and `map_Bump` hold normal maps, as Blender writes
    /// them, rather than heights, as MTL says (D19).
    pub normal_maps: bool,
}

/// `library`'s materials as networks, each keyed by its name, and what was
/// left out, said once a material.
pub fn materials(
    library: &MaterialLibrary,
    maps: Maps<'_>,
) -> (Vec<ImportedMaterial>, Vec<Diagnostic>) {
    let mut warnings = Vec::new();
    let made = (library.materials.iter())
        .map(|m| {
            let mut build = Build {
                seed: NetworkSeed::default(),
                maps,
                images: Vec::new(),
                left: Vec::new(),
                warnings: &mut warnings,
            };
            build.material(m);
            let Build { seed, left, .. } = build;
            if !left.is_empty() {
                let left = left.join(", ");
                warnings.push(Diagnostic::warning(format!("{}: left out {left}", m.name)));
            }
            ImportedMaterial {
                key: m.name.clone(),
                name: m.name.clone(),
                inside: seed,
                standard: None,
                // MTL records no sides: both show, as every reader shows them.
                double_sided: true,
            }
        })
        .collect();
    (made, warnings)
}

/// The type a map reads as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Read {
    Color,
    Float,
    Vector,
}

/// One material's network as it is made.
struct Build<'a, 'w> {
    seed: NetworkSeed,
    maps: Maps<'a>,
    /// The Image node reading each file, by its path as written.
    images: Vec<(String, usize)>,
    /// What was left out, in words (`map_Kd -blendu`, `sharpness`).
    left: Vec<String>,
    warnings: &'w mut Vec<Diagnostic>,
}

impl Build<'_, '_> {
    /// A MaterialX node of `signature`; its index.
    fn node(&mut self, signature: &str) -> usize {
        let node = NewNode::materialx(signature).expect("MaterialX's library defines it");
        self.seed.node(node)
    }

    fn set(&mut self, at: usize, input: &str, value: Value) {
        self.seed.nodes[at]
            .parameters
            .insert(input.to_owned(), value);
    }

    /// `from`'s output into `to`'s `input`.
    fn wire(&mut self, from: usize, to: usize, input: &str) {
        self.seed.wire(from, "out", to, input);
    }

    fn material(&mut self, m: &Material) {
        let pbr = PBR_ONLY.iter().any(|k| holds(m, k));
        let surface = match pbr {
            true => {
                let surface = self.node(STANDARD);
                for (color, weight) in WEIGHTS {
                    let s = STANDARD_STATEMENTS.iter().find(|s| s.input == color);
                    if s.is_some_and(|s| holds(m, s.keyword) || holds(m, s.map)) {
                        self.set(surface, weight, Value::Number(1.0));
                    }
                }
                for s in &STANDARD_STATEMENTS {
                    self.statement(m, s, surface);
                }
                let classic: Vec<&Statement> =
                    PHONG_STATEMENTS.iter().filter(|s| beside(s)).collect();
                let held = |s: &&Statement| holds(m, s.keyword) || holds(m, s.map);
                if classic.iter().any(held) || m.transparency.is_some() {
                    let phong = self.node(PHONG);
                    for s in classic.into_iter().filter(held) {
                        self.statement(m, s, phong);
                    }
                    if let Some(tr) = m.transparency {
                        self.set(phong, "transparency_factor", Value::Number(tr));
                    }
                }
                surface
            }
            false => {
                let phong = self.node(PHONG);
                for s in &PHONG_STATEMENTS {
                    self.statement(m, s, phong);
                }
                // Tr is the opacity's inverse where nothing else gives it.
                match (m.transparency, holds(m, "d") || holds(m, "map_d")) {
                    (Some(tr), true) => self.set(phong, "transparency_factor", Value::Number(tr)),
                    (Some(tr), false) => {
                        let invert = self.node(sig::INVERT);
                        self.set(invert, "in", Value::Number(tr));
                        self.wire(invert, phong, "opacity");
                    }
                    (None, _) => {}
                }
                phong
            }
        };
        self.normal(m, surface);
        match m.map(DISPLACEMENT) {
            Some(map) => {
                let displacement = self.node(sig::DISPLACEMENT);
                let heights = self.texture(map, Read::Float);
                self.wire(heights, displacement, "displacement");
                let material = self.node(sig::SURFACE_MATERIAL);
                self.seed.nodes[material].display = true;
                self.wire(surface, material, "surfaceshader");
                self.wire(displacement, material, "displacementshader");
            }
            None => self.seed.nodes[surface].display = true,
        }
        self.left_out(m);
    }

    /// Statement `s` of `m` into `surface`: its value, its map, or the map
    /// times the value.
    fn statement(&mut self, m: &Material, s: &Statement, surface: usize) {
        let value = held(m, s.keyword);
        let map = (!s.map.is_empty()).then(|| m.map(s.map)).flatten();
        let Some(map) = map else {
            if let Some(v) = value {
                self.set(surface, s.input, value_of(v, s.kind));
            }
            return;
        };
        let read = match s.kind {
            Kind::Color => Read::Color,
            _ => Read::Float,
        };
        let mut out = self.texture(map, read);
        if let Some(v) = value {
            let (signature, v) = match s.kind {
                Kind::Color => (sig::MULTIPLY_COLOR, value_of(v, Kind::Color)),
                _ => (sig::MULTIPLY, value_of(v, Kind::Number)),
            };
            let times = self.node(signature);
            self.set(times, "in2", v);
            self.wire(out, times, "in1");
            out = times;
        }
        if s.kind == Kind::Grey {
            let grey = self.node(sig::GREY);
            self.wire(out, grey, "in");
            out = grey;
        }
        self.wire(out, surface, s.input);
    }

    /// The material's normal map and bump map into `surface`'s normal, the
    /// bump reading the normal map when it has both.
    fn normal(&mut self, m: &Material, surface: usize) {
        let mut normal = m.map(NORMAL).map(|map| self.normal_map(map));
        let bump = m
            .maps
            .iter()
            .find(|map| BUMP.iter().any(|k| map.kind.eq_ignore_ascii_case(k)));
        if let Some(map) = bump {
            if self.maps.normal_maps {
                match normal {
                    Some(_) => self
                        .left
                        .push(format!("{} (a second normal map)", map.kind)),
                    None => normal = Some(self.normal_map(map)),
                }
            } else {
                let looks = Path::new(&map.path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase());
                if looks.is_some_and(|s| s.contains("normal") || s.contains("nrm")) {
                    self.warnings.push(Diagnostic::warning(format!(
                        "{}: {} {} reads as heights, as MTL says; its name says a normal map",
                        m.name, map.kind, map.path
                    )));
                }
                let bump = self.node(sig::BUMP);
                let heights = self.texture(map, Read::Float);
                self.wire(heights, bump, "height");
                if let Some(scale) = map.options.bump {
                    self.set(bump, "scale", Value::Number(scale));
                }
                if let Some(normal) = normal {
                    self.wire(normal, bump, "normal");
                }
                normal = Some(bump);
            }
        }
        if let Some(normal) = normal {
            self.wire(normal, surface, "normal");
        }
    }

    /// A `normalmap` of `map`, its `-bm` the scale.
    fn normal_map(&mut self, map: &TextureMap) -> usize {
        let normal = self.node(sig::NORMAL_MAP);
        let read = self.texture(map, Read::Vector);
        self.wire(read, normal, "in");
        if let Some(scale) = map.options.bump {
            self.set(normal, "scale", Value::Number(scale));
        }
        normal
    }

    /// `map` read as `read`: its Image node into an `image` node, through
    /// its options. The node giving the value.
    fn texture(&mut self, map: &TextureMap, read: Read) -> usize {
        let o = &map.options;
        let channel = match (read, o.channel) {
            (Read::Float, Some(c @ ('r' | 'g' | 'b'))) => {
                Some((sig::EXTRACT_COLOR3, "rgb".find(c)))
            }
            (Read::Float, Some('m')) => Some((sig::EXTRACT_COLOR4, Some(3))),
            (_, Some(c)) => {
                self.left.push(format!("{} -imfchan {c}", map.kind));
                None
            }
            (_, None) => None,
        };
        let signature = match (read, channel) {
            (_, Some((sig::EXTRACT_COLOR4, _))) => "ND_image_color4",
            (Read::Color, _) | (_, Some(_)) => "ND_image_color3",
            (Read::Float, None) => "ND_image_float",
            (Read::Vector, None) => "ND_image_vector3",
        };
        let file = self.image(&map.path);
        let image = self.node(signature);
        self.seed.wire(file, image_node::IMAGE, image, "file");
        if o.clamp == Some(true) {
            for mode in ["uaddressmode", "vaddressmode"] {
                self.set(image, mode, Value::Choice(Arc::from("clamp")));
            }
        }
        let scale = o.scale.filter(|s| s[..2] != [1.0, 1.0]);
        let offset = o.offset.filter(|s| s[..2] != [0.0, 0.0]);
        if o.scale.is_some_and(|s| s[2] != 1.0) || o.offset.is_some_and(|s| s[2] != 0.0) {
            self.left
                .push(format!("{}'s third texture coordinate", map.kind));
        }
        if scale.is_some() || offset.is_some() {
            let mut at = self.node(sig::TEXCOORD);
            for (signature, by) in [(sig::MULTIPLY_UV, scale), (sig::ADD_UV, offset)] {
                if let Some(by) = by {
                    let step = self.node(signature);
                    self.set(step, "in2", Value::Numbers(vec![by[0], by[1]]));
                    self.wire(at, step, "in1");
                    at = step;
                }
            }
            self.wire(at, image, "texcoord");
        }
        let mut out = image;
        if let Some((signature, Some(index))) = channel {
            let extract = self.node(signature);
            self.set(extract, "index", Value::Integer(index as i64));
            self.wire(image, extract, "in");
            out = extract;
        }
        if let Some([base, gain]) = o.range.filter(|r| *r != [0.0, 1.0]) {
            let (times, plus) = match read {
                Read::Color => (sig::MULTIPLY_COLOR_BY, sig::ADD_COLOR_BY),
                _ => (sig::MULTIPLY, sig::ADD),
            };
            let times = self.node(times);
            self.set(times, "in2", Value::Number(gain));
            self.wire(out, times, "in1");
            let plus = self.node(plus);
            self.set(plus, "in2", Value::Number(base));
            self.wire(times, plus, "in1");
            out = plus;
        }
        let bump = BUMP
            .iter()
            .chain([&NORMAL])
            .any(|k| map.kind.eq_ignore_ascii_case(k));
        if o.bump.is_some() && !bump {
            self.left.push(format!("{} -bm", map.kind));
        }
        for (option, _) in &o.other {
            self.left.push(format!("{} {option}", map.kind));
        }
        out
    }

    /// The Image node reading `path` (relative to the library's folder): one
    /// a file, its colors as the file says.
    fn image(&mut self, path: &str) -> usize {
        if let Some((_, at)) = self.images.iter().find(|(p, _)| p == path) {
            return *at;
        }
        let whole = match self.maps.folder {
            Some(folder) => folder.join(path),
            None => path.into(),
        };
        let mut file = ImageFile::Path(Arc::from(whole.display().to_string()));
        if self.maps.embed {
            match std::fs::read(&whole) {
                Ok(bytes) => {
                    let name =
                        (whole.file_name()).map_or_else(|| path.into(), |n| n.to_string_lossy());
                    file = ImageFile::Embedded(EmbeddedFile::new(&name, bytes));
                }
                Err(e) => self.warnings.push(Diagnostic::warning(format!(
                    "{}: {e}, kept as a path",
                    whole.display()
                ))),
            }
        }
        let stem = Path::new(path).file_stem();
        let name = stem.map_or_else(|| "Image".into(), |s| s.to_string_lossy());
        let source = ImageSource::new(file, Default::default());
        let at = self.seed.node(image_node::reading(&source).named(name));
        self.images.push((path.to_owned(), at));
        at
    }

    /// What `m` holds that no statement above reads, in words.
    fn left_out(&mut self, m: &Material) {
        let read = |kind: &str| {
            let tables = PHONG_STATEMENTS.iter().chain(&STANDARD_STATEMENTS);
            let mapped = tables.map(|s| s.map).filter(|k| !k.is_empty());
            let special = BUMP.iter().copied().chain([NORMAL, DISPLACEMENT]);
            mapped.chain(special).any(|k| k.eq_ignore_ascii_case(kind))
        };
        let mut seen: Vec<String> = Vec::new();
        for map in &m.maps {
            let kind = map.kind.to_ascii_lowercase();
            let bump = BUMP.iter().any(|k| k.eq_ignore_ascii_case(&kind));
            let first = !seen
                .iter()
                .any(|s| *s == kind || (bump && BUMP.iter().any(|k| k.eq_ignore_ascii_case(s))));
            if !read(&kind) || !first {
                self.left.push(map.kind.clone());
            }
            seen.push(kind);
        }
        self.left
            .extend(m.other.iter().map(|(keyword, _)| keyword.clone()));
    }
}

/// A statement's value as `m` holds it.
#[derive(Clone, Copy, Debug)]
enum Held {
    Color([f64; 3]),
    Number(f64),
}

/// Statement `keyword`'s value in `m`; none for a map or one it doesn't hold.
fn held(m: &Material, keyword: &str) -> Option<Held> {
    use Held::{Color, Number};
    match keyword {
        "Ka" => m.ambient.map(Color),
        "Kd" => m.diffuse.map(Color),
        "Ks" => m.specular.map(Color),
        "Ke" => m.emissive.map(Color),
        "Tf" => m.transmission_filter.map(Color),
        "Ns" => m.shininess.map(Number),
        "Ni" => m.optical_density.map(Number),
        "d" => m.dissolve.map(Number),
        "illum" => m.illumination.map(|n| Number(f64::from(n))),
        "Pr" => m.roughness.map(Number),
        "Pm" => m.metallic.map(Number),
        "Ps" => m.sheen.map(Number),
        "Pc" => m.clearcoat.map(Number),
        "Pcr" => m.clearcoat_roughness.map(Number),
        "aniso" => m.anisotropy.map(Number),
        "anisor" => m.anisotropy_rotation.map(Number),
        _ => None,
    }
}

/// Whether `m` holds statement or map `keyword`.
fn holds(m: &Material, keyword: &str) -> bool {
    !keyword.is_empty() && (held(m, keyword).is_some() || m.map(keyword).is_some())
}

/// `v` as an input of `kind` holds it.
fn value_of(v: Held, kind: Kind) -> Value {
    match (v, kind) {
        (Held::Color(c), _) => Value::Color(c),
        (Held::Number(n), Kind::Grey) => Value::Color([n; 3]),
        (Held::Number(n), Kind::Integer) => Value::Integer(n as i64),
        (Held::Number(n), _) => Value::Number(n),
    }
}
