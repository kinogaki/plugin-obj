//! Materials as a material library (the module docs say how each input
//! maps): a Phong Surface's and a Standard Surface's statements as their
//! networks record them, any other model through Standard Surface.
use super::{
    BUMP, DISPLACEMENT, Kind, NORMAL, PHONG_STATEMENTS, SPACE, STANDARD_STATEMENTS, Statement,
    WEIGHTS, beside,
};
use crate::obj::mtl::{MapOptions, Material, MaterialLibrary, TextureMap};
use kinogaki_render_api::MaterialNetwork;
use kinogaki_render_api::material::mtlx::{SIGNATURE, def};
use kinogaki_studio_api::color::{ColorService, settled};
use kinogaki_studio_api::format::files::Beside;
use kinogaki_studio_api::image_files::{IMAGE_NODE, image_source};
use kinogaki_studio_api::network::{NetworkSeed, SeedWire};
use kinogaki_studio_api::standard::{ImportedMaterial, Materials};
use kinogaki_studio_api::{Diagnostic, Error, Value};
use std::collections::BTreeMap;

/// A library made of materials: what is written, the name each material's
/// key takes in it, and what was written with loss, said.
pub struct Library {
    pub library: MaterialLibrary,
    pub names: BTreeMap<String, String>,
    pub warnings: Vec<Diagnostic>,
}

/// `held`'s materials as a library, each named as it is (numbered past a
/// name taken), their colors out of `colors`' working space, their images
/// named by `files`.
///
/// # Errors
/// An embedded image that can't be written, a color that can't be
/// converted.
pub fn library(
    held: &Materials,
    files: &mut Beside,
    colors: &dyn ColorService,
) -> Result<Library, Error> {
    let mut made = Library {
        library: MaterialLibrary::default(),
        names: BTreeMap::new(),
        warnings: Vec::new(),
    };
    for m in &held.materials {
        let mut name = m.name.replace(['\n', '\r', '#'], " ").trim().to_owned();
        let mut n = 1;
        while made.library.get(&name).is_some() {
            n += 1;
            name = format!("{} {n}", m.name.trim());
        }
        let mut written = Written {
            material: Material {
                name: name.clone(),
                ..Material::default()
            },
            files: &mut *files,
            colors,
            lossy: Vec::new(),
        };
        written.material_of(m)?;
        if !written.lossy.is_empty() {
            let lossy = written.lossy.join(", ");
            (made.warnings).push(Diagnostic::warning(format!("{name}: {lossy}")));
        }
        made.library.materials.push(written.material);
        made.names.insert(m.key.clone(), name);
    }
    Ok(made)
}

/// A network read node by node: what drives an input, what a node is.
struct Net<'a> {
    seed: &'a NetworkSeed,
}

impl Net<'_> {
    /// The wire into `at`'s `input`.
    fn into(&self, at: usize, input: &str) -> Option<&SeedWire> {
        self.seed
            .wires
            .iter()
            .find(|w| w.to == at && w.input == input)
    }

    /// The node feeding `at`'s `input`.
    fn from(&self, at: usize, input: &str) -> Option<usize> {
        self.into(at, input).map(|w| w.from)
    }

    /// Node `at`'s signature.
    fn signature(&self, at: usize) -> Option<&str> {
        self.seed.nodes[at].parameters.get(SIGNATURE)?.choice()
    }

    /// Whether node `at` is MaterialX's `node`.
    fn is(&self, at: usize, node: &str) -> bool {
        let def = self.signature(at).and_then(def);
        def.is_some_and(|d| d.mtlx().is_some_and(|m| m.node == node))
    }

    /// The value `at` holds at `input`, unwired.
    fn value(&self, at: usize, input: &str) -> Option<&Value> {
        match self.into(at, input) {
            Some(_) => None,
            None => self.seed.nodes[at].parameters.get(input),
        }
    }

    /// The shown node: a surface, or a Surface Material.
    fn shown(&self) -> Option<usize> {
        self.seed.nodes.iter().position(|n| n.display)
    }
}

/// One material as it is written.
struct Written<'f> {
    material: Material,
    files: &'f mut Beside,
    colors: &'f dyn ColorService,
    /// What was written with loss, in words.
    lossy: Vec<String>,
}

impl Written<'_> {
    fn material_of(&mut self, m: &ImportedMaterial) -> Result<(), Error> {
        if !m.double_sided {
            self.lossy
                .push("single sided, which MTL doesn't record".into());
        }
        let source = match (&m.standard, all_materialx(&m.inside)) {
            (_, true) => m.inside.clone(),
            (Some(standard), false) => standard.clone(),
            (None, false) => {
                self.lossy
                    .push("nothing standard approximates it, written without a look".into());
                return Ok(());
            }
        };
        let net = Net { seed: &source };
        let Some(shown) = net.shown() else {
            return Ok(());
        };
        let material = net.is(shown, "surfacematerial");
        let surface = match material {
            true => net.from(shown, "surfaceshader"),
            false => Some(shown),
        };
        let Some(surface) = surface else {
            return Ok(());
        };
        if net.is(surface, "kinogaki_phong_surface") {
            self.phong(&net, surface, &PHONG_STATEMENTS)?;
        } else if net.is(surface, "standard_surface") {
            self.standard(&net, surface)?;
        } else {
            // Another model: translated to Standard Surface, its textures as
            // they are.
            let translated = MaterialNetwork::try_from(&source)
                .ok()
                .and_then(|n| n.translated("standard_surface"));
            let Some(translated) = translated else {
                self.lossy.push(
                    "its surface doesn't translate to Standard Surface, written without a look"
                        .into(),
                );
                return Ok(());
            };
            let seed = NetworkSeed::from(&translated);
            let net = Net { seed: &seed };
            let surface = net
                .shown()
                .and_then(|s| match net.is(s, "surfacematerial") {
                    true => net.from(s, "surfaceshader"),
                    false => Some(s),
                });
            if let Some(surface) = surface {
                self.standard(&net, surface)?;
            }
            return Ok(());
        }
        let displacement = (material
            .then(|| net.from(shown, "displacementshader"))
            .flatten())
        .filter(|&d| net.is(d, "displacement"));
        if let Some(displacement) = displacement
            && let Some(map) =
                self.texture(&net, net.from(displacement, "displacement"), DISPLACEMENT)?
        {
            self.material.maps.push(map);
        }
        Ok(())
    }

    /// The Phong Surface `at`'s statements of `statements`, and its `Tr`.
    fn phong(&mut self, net: &Net<'_>, at: usize, statements: &[Statement]) -> Result<(), Error> {
        let inverted = net.from(at, "opacity").filter(|&i| net.is(i, "invert"));
        for s in statements {
            if s.keyword == "d" && inverted.is_some() {
                continue;
            }
            self.statement(net, at, s, 1.0)?;
        }
        let tr = match inverted {
            Some(invert) => net.value(invert, "in").and_then(Value::number),
            None => net.value(at, "transparency_factor").and_then(Value::number),
        };
        self.material.transparency = tr.or(self.material.transparency);
        self.normal(net, at)
    }

    /// The Standard Surface `at`'s statements, and the classic ones of a
    /// Phong Surface beside it.
    fn standard(&mut self, net: &Net<'_>, at: usize) -> Result<(), Error> {
        for s in &STANDARD_STATEMENTS {
            let weight = WEIGHTS.iter().find(|(color, _)| *color == s.input);
            let weight = weight.and_then(|(_, w)| net.value(at, w)?.number());
            self.statement(net, at, s, weight.unwrap_or(1.0))?;
        }
        let phong =
            (0..net.seed.nodes.len()).find(|&n| n != at && net.is(n, "kinogaki_phong_surface"));
        if let Some(phong) = phong {
            let classic: Vec<Statement> = PHONG_STATEMENTS
                .iter()
                .filter(|s| beside(s))
                .copied()
                .collect();
            for s in &classic {
                self.statement(net, phong, s, 1.0)?;
            }
            let tr = net
                .value(phong, "transparency_factor")
                .and_then(Value::number);
            self.material.transparency = tr;
        }
        self.normal(net, at)
    }

    /// Statement `s` from what drives `at`'s input: its value (times
    /// `weight`), its map, or both where a map is multiplied by a value.
    fn statement(
        &mut self,
        net: &Net<'_>,
        at: usize,
        s: &Statement,
        weight: f64,
    ) -> Result<(), Error> {
        let mut from = net.from(at, s.input);
        if s.kind == Kind::Grey && from.is_some_and(|f| net.is(f, "convert")) {
            from = from.and_then(|f| net.from(f, "in"));
        }
        let mut value = match from {
            Some(_) => None,
            None => net.seed.nodes[at].parameters.get(s.input).cloned(),
        };
        if let Some(f) = from
            && net.is(f, "multiply")
            && net.into(f, "in1").is_some()
            && let Some(v) = net.value(f, "in2")
        {
            value = Some(v.clone());
            from = net.from(f, "in1");
        }
        if let Some(v) = value
            .as_ref()
            .map(|v| self.held(v, s, weight))
            .transpose()?
            .flatten()
        {
            set(&mut self.material, s.keyword, v);
        } else if from.is_some() && weight != 1.0 && !s.map.is_empty() {
            set(&mut self.material, s.keyword, Held::Color([weight; 3]));
        }
        if !s.map.is_empty()
            && let Some(map) = self.texture(net, from, s.map)?
        {
            self.material.maps.push(map);
        } else if from.is_some() && value.is_none() {
            self.lossy.push(format!(
                "{} from a node MTL can't record, left out",
                s.keyword
            ));
        }
        Ok(())
    }

    /// `v`, statement `s`'s value, times `weight`, as the library holds it:
    /// a color out of the working space.
    fn held(&mut self, v: &Value, s: &Statement, weight: f64) -> Result<Option<Held>, Error> {
        let held = match (v, s.kind) {
            (Value::Color(c), Kind::Grey) if c[0] == c[1] && c[1] == c[2] => Held::Number(c[0]),
            (Value::Color(c), Kind::Grey) => {
                (self.lossy).push(format!("{} the mean of its color", s.keyword));
                Held::Number((c[0] + c[1] + c[2]) / 3.0)
            }
            (Value::Color(c), _) => {
                let mut rgb = [c.map(|x| x * weight)];
                self.colors.working_to(SPACE, &mut rgb)?;
                Held::Color(rgb[0].map(settled))
            }
            (Value::Number(n), Kind::Color) => Held::Color([n * weight; 3]),
            (Value::Number(n), _) => Held::Number(*n),
            (Value::Integer(n), _) => Held::Number(*n as f64),
            _ => return Ok(None),
        };
        Ok(Some(held))
    }

    /// The bump and normal maps driving `at`'s normal.
    fn normal(&mut self, net: &Net<'_>, at: usize) -> Result<(), Error> {
        let mut from = net.from(at, "normal");
        if let Some(bump) = from.filter(|&b| net.is(b, "bump")) {
            if let Some(mut map) = self.texture(net, net.from(bump, "height"), BUMP[0])? {
                map.options.bump = net
                    .value(bump, "scale")
                    .and_then(Value::number)
                    .filter(|s| *s != 1.0);
                self.material.maps.push(map);
            }
            from = net.from(bump, "normal");
        }
        if let Some(normal) = from.filter(|&n| net.is(n, "normalmap"))
            && let Some(mut map) = self.texture(net, net.from(normal, "in"), NORMAL)?
        {
            map.options.bump = net
                .value(normal, "scale")
                .and_then(Value::number)
                .filter(|s| *s != 1.0);
            self.material.maps.push(map);
        }
        Ok(())
    }

    /// Map `kind` of what node `from` gives: the image it reads, through the
    /// options a texture chain records. None when no image is read.
    fn texture(
        &mut self,
        net: &Net<'_>,
        from: Option<usize>,
        kind: &str,
    ) -> Result<Option<TextureMap>, Error> {
        let Some(mut at) = from else {
            return Ok(None);
        };
        let mut options = MapOptions::default();
        // `-mm base gain`: a multiply by the gain, then an add of the base.
        if net.is(at, "add")
            && let Some(base) = net.value(at, "in2").and_then(Value::number)
            && let Some(times) = net.from(at, "in1").filter(|&t| net.is(t, "multiply"))
            && let Some(gain) = net.value(times, "in2").and_then(Value::number)
            && let Some(read) = net.from(times, "in1")
        {
            options.range = Some([base, gain]);
            at = read;
        }
        if net.is(at, "extract")
            && let Some(read) = net.from(at, "in")
        {
            let index = net.value(at, "index").and_then(Value::integer).unwrap_or(0);
            let alpha = net.signature(read) == Some("ND_image_color4") && index == 3;
            options.channel = match alpha {
                true => Some('m'),
                false => "rgb".chars().nth(index as usize),
            };
            at = read;
        }
        // The node reading an image: this one, else the first upstream.
        let mut reader = at;
        let mut steps = 0;
        let file = loop {
            if let Some(w) = net.into(reader, "file")
                && net.seed.nodes[w.from].node_type == IMAGE_NODE
            {
                break image_source(&net.seed.nodes[w.from].parameters)?;
            }
            let upstream = net.seed.wires.iter().find(|w| w.to == reader);
            match upstream {
                Some(w) if steps < 64 => {
                    reader = w.from;
                    steps += 1;
                }
                _ => return Ok(None),
            }
        };
        let Some(source) = file else {
            self.lossy.push(format!("{kind} reads a result, left out"));
            return Ok(None);
        };
        if net.is(reader, "image") {
            let clamped =
                |mode: &str| net.value(reader, mode).and_then(Value::choice) == Some("clamp");
            if clamped("uaddressmode") && clamped("vaddressmode") {
                options.clamp = Some(true);
            }
            let mut uv = net.from(reader, "texcoord");
            if let Some(add) = uv.filter(|&a| net.is(a, "add"))
                && let Some(o) = net.value(add, "in2").and_then(pair)
            {
                options.offset = Some([o[0], o[1], 0.0]);
                uv = net.from(add, "in1");
            }
            if let Some(times) = uv.filter(|&t| net.is(t, "multiply"))
                && let Some(s) = net.value(times, "in2").and_then(pair)
            {
                options.scale = Some([s[0], s[1], 1.0]);
            }
        } else if reader != at || !net.is(reader, "image") {
            self.lossy
                .push(format!("{kind} read through a node MTL can't record"));
        }
        let path = self.files.name(source.file())?;
        Ok(Some(TextureMap {
            kind: kind.to_owned(),
            options,
            path,
        }))
    }
}

/// A vector2's two numbers.
fn pair(v: &Value) -> Option<[f64; 2]> {
    match v {
        Value::Numbers(n) if n.len() == 2 => Some([n[0], n[1]]),
        _ => None,
    }
}

/// Whether every node of `seed` is MaterialX's or an Image node: what a
/// library can record as it is.
fn all_materialx(seed: &NetworkSeed) -> bool {
    (seed.nodes.iter()).all(|n| n.node_type == IMAGE_NODE || n.parameters.contains_key(SIGNATURE))
}

/// A statement's value as written.
#[derive(Clone, Copy, Debug)]
enum Held {
    Color([f64; 3]),
    Number(f64),
}

/// Statement `keyword` of `m` set to `v`.
fn set(m: &mut Material, keyword: &str, v: Held) {
    let color = match v {
        Held::Color(c) => c,
        Held::Number(n) => [n; 3],
    };
    let number = match v {
        Held::Color(c) => c[0],
        Held::Number(n) => n,
    };
    match keyword {
        "Ka" => m.ambient = Some(color),
        "Kd" => m.diffuse = Some(color),
        "Ks" => m.specular = Some(color),
        "Ke" => m.emissive = Some(color),
        "Tf" => m.transmission_filter = Some(color),
        "Ns" => m.shininess = Some(number),
        "Ni" => m.optical_density = Some(number),
        "d" => m.dissolve = Some(number),
        "illum" => m.illumination = Some(number.max(0.0) as u32),
        "Pr" => m.roughness = Some(number),
        "Pm" => m.metallic = Some(number),
        "Ps" => m.sheen = Some(number),
        "Pc" => m.clearcoat = Some(number),
        "Pcr" => m.clearcoat_roughness = Some(number),
        "aniso" => m.anisotropy = Some(number),
        "anisor" => m.anisotropy_rotation = Some(number),
        _ => {}
    }
}
