//! MTL materials through MaterialX: every statement and option DESIGN §6.5
//! maps reads into its network and writes back as the same statement and
//! values; Blender's exports read and write back unchanged.
use kinogaki_obj::Obj;
use kinogaki_obj::obj::mtl::{self, Material, MaterialLibrary};
use kinogaki_render_api::MaterialNetwork;
use kinogaki_render_api::material::mtlx::SIGNATURE;
use kinogaki_studio_api::format::options::EMBED;
use kinogaki_studio_api::geometry::image::ImageFile;
use kinogaki_studio_api::geometry::{Attribute, Domain, Geometry, Text, names};
use kinogaki_studio_api::image_files::{IMAGE_NODE, image_source};
use kinogaki_studio_api::network::NetworkSeed;
use kinogaki_studio_api::standard::{self, Materials};
use kinogaki_studio_api::{
    Diagnostic, FileFormat, FileItem, FormatOptions, Input, Purpose, Reading, Units, Value, Writing,
};
use kinogaki_studio_api_testing::Unconverted;
use kinogaki_studio_api_testing::format::Scratch;
use std::path::Path;

/// A square naming material `key`.
fn square(key: &str) -> Geometry {
    use kinogaki_studio_api::geometry::{Attributes, Primitives};
    let table = Primitives::of_polygons(4, vec![0, 4].into(), vec![0, 1, 2, 3].into()).unwrap();
    let at = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let text = Text::from_strs(std::iter::once(key));
    let attributes = Attributes::new()
        .with(Domain::Primitive, names::MATERIAL, Attribute::text(text))
        .unwrap();
    Geometry::new(at.into(), table, attributes).unwrap()
}

/// `obj` (an OBJ file at `path`, its library beside it) read with `set`
/// over the defaults: its materials and the read's warnings.
fn read(path: &Path, set: &[(&str, Value)]) -> (Materials, Vec<Diagnostic>) {
    let mut options = Obj.options();
    options.extend(set.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    let colors = Unconverted;
    let reading = Reading::new(Purpose::Objects, Units::METERS, &colors);
    let read = Obj.read(&Input::file(path), &options, &reading).unwrap();
    let data = read.items[0].data.as_ref().expect("the materials first");
    let held = standard::materials().get(data).expect("materials").clone();
    (held, read.warnings)
}

/// `library` written beside an OBJ naming its first material in a new
/// folder, read with `set`.
fn read_library(name: &str, library: &str, set: &[(&str, Value)]) -> (Materials, Vec<Diagnostic>) {
    let dir = Scratch::new(name);
    std::fs::write(dir.join("lib.mtl"), library).unwrap();
    let obj = "mtllib lib.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    std::fs::write(dir.join("model.obj"), obj).unwrap();
    read(&dir.join("model.obj"), set)
}

/// `held` written as `<dir>/<name>.obj` with a square naming each material:
/// its library as written, and the writer's warnings.
fn write(dir: &Path, name: &str, held: &Materials) -> (MaterialLibrary, Vec<Diagnostic>) {
    let mut items = vec![FileItem::top(
        "Materials",
        standard::materials().wrap(held.clone()),
    )];
    for m in &held.materials {
        items.push(FileItem::top(
            &m.name,
            standard::geometry().wrap(square(&m.key)),
        ));
    }
    let colors = Unconverted;
    let writing = Writing::new(Units::METERS, &colors);
    let file = dir.join(format!("{name}.obj"));
    let writer = Obj.writer().expect("it writes");
    let warnings = writer
        .write(&file, &items, &writing, &FormatOptions::new())
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains(&format!("mtllib {name}.mtl")), "{text}");
    (
        mtl::read_file(dir.join(format!("{name}.mtl"))).unwrap(),
        warnings,
    )
}

/// `library` as statements compare: each material's maps in one order, a
/// height map's two keywords as one.
fn normalized(mut library: MaterialLibrary) -> MaterialLibrary {
    for m in &mut library.materials {
        for map in &mut m.maps {
            if map.kind.eq_ignore_ascii_case("bump") || map.kind.eq_ignore_ascii_case("map_bump") {
                map.kind = "map_Bump".into();
            }
        }
        m.maps.sort_by(|a, b| a.kind.cmp(&b.kind));
    }
    library
}

/// The MaterialX node of `seed` holding signature `signature`.
fn node(seed: &NetworkSeed, signature: &str) -> Vec<usize> {
    (seed.nodes.iter().enumerate())
        .filter(|(_, n)| n.parameters.get(SIGNATURE).and_then(Value::choice) == Some(signature))
        .map(|(at, _)| at)
        .collect()
}

/// What drives `at`'s `input`.
fn into(seed: &NetworkSeed, at: usize, input: &str) -> usize {
    let wire = seed.wires.iter().find(|w| w.to == at && w.input == input);
    wire.unwrap_or_else(|| panic!("nothing into {input}")).from
}

/// Every statement and option DESIGN §6.5 maps, in the classic model and in
/// the PBR extension's, the classic statements beside it too.
const EVERY: &str = "\
newmtl classic
Ka 0.1 0.2 0.3
Kd 0.5 0.6 0.7
Ks 0.25 0.25 0.25
Ke 0.01 0.02 0.03
Tf 0.9 1 0.9
Ns 96
Ni 1.45
d 0.75
Tr 0.25
illum 2
map_Ka ambient.png
map_Kd -o 0.25 0.5 0 -s 2 3 1 -clamp on wood.png
map_Ks spec.png
map_Ke -mm 0.1 2 glow.png
map_Ns -imfchan r rough.png
map_d -imfchan m alpha.png
map_refl metal.png
map_Bump -bm 0.5 height.png
disp -mm 0 0.1 height.png

newmtl pbr
Kd 0.8 0.7 0.6
Ke 1 0.5 0.25
Ni 1.5
d 0.5
Tf 1 0.9 0.8
Pr 0.3
Pm 1
Ps 0.2
Pc 0.4
Pcr 0.05
aniso 0.6
anisor 0.125
Ka 1 1 1
Ks 0.5 0.5 0.5
Ns 250
Tr 0.5
illum 3
map_Kd wood.png
map_Ke glow.png
map_d -imfchan g alpha.png
map_Pr -imfchan b rough.png
map_Pm metal.png
map_Ps sheen.png
map_Pc coat.png
map_Pcr coat_rough.png
map_aniso aniso.png
map_anisor rotation.png
map_Ka ambient.png
map_Ks spec.png
map_Ns rough.png
map_refl metal.png
norm -bm 0.8 normal.png
bump -bm 0.25 height.png

newmtl only_tr
Kd 1 0 0
Tr 0.6
";

#[test]
fn every_statement_and_option_writes_back_as_it_was_read() {
    let (held, warnings) = read_library("every", EVERY, &[]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(held.materials.len(), 3);
    for m in &held.materials {
        // Every network is one MaterialX takes: its types and values check.
        MaterialNetwork::try_from(&m.inside).unwrap_or_else(|e| panic!("{}: {e}", m.name));
        assert_eq!(
            (m.key.as_str(), m.standard.is_none()),
            (m.name.as_str(), true)
        );
    }
    let dir = Scratch::new("every-written");
    let (written, warnings) = write(&dir, "every", &held);
    assert!(warnings.is_empty(), "{warnings:?}");
    let read = mtl::parse(EVERY.as_bytes()).unwrap();
    // The texture files are named relative to the library they were read
    // beside, which is now elsewhere.
    let mut written = normalized(written);
    for m in &mut written.materials {
        for map in &mut m.maps {
            map.path = Path::new(&map.path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into();
        }
    }
    assert_eq!(written, normalized(read));
}

#[test]
fn classic_statements_are_a_phong_surface_and_maps_image_nodes_through_their_options() {
    let (held, _) = read_library("classic", EVERY, &[]);
    let seed = &held.materials[0].inside;
    let [phong] = node(seed, "ND_kinogaki_phong_surface_surfaceshader")[..] else {
        panic!("one Phong Surface")
    };
    let params = &seed.nodes[phong].parameters;
    assert_eq!(params["transmission_filter"], Value::Color([0.9, 1.0, 0.9]));
    assert_eq!(params["ior"], Value::Number(1.45));
    assert_eq!(params["illum"], Value::Integer(2));
    // d and Tr both: Tr is its Transparency Factor.
    assert_eq!(params["transparency_factor"], Value::Number(0.25));
    // Kd times map_Kd, read through -s and -o, clamped.
    let times = into(seed, phong, "diffuse_color");
    assert_eq!(
        seed.nodes[times].parameters["in2"],
        Value::Color([0.5, 0.6, 0.7])
    );
    let image = into(seed, times, "in1");
    assert_eq!(
        seed.nodes[image].parameters["uaddressmode"],
        Value::Choice("clamp".into())
    );
    let moved = into(seed, image, "texcoord");
    assert_eq!(
        seed.nodes[moved].parameters["in2"],
        Value::Numbers(vec![0.25, 0.5])
    );
    let scaled = into(seed, moved, "in1");
    assert_eq!(
        seed.nodes[scaled].parameters["in2"],
        Value::Numbers(vec![2.0, 3.0])
    );
    assert_eq!(
        node(seed, "ND_texcoord_vector2"),
        [into(seed, scaled, "in1")]
    );
    let file = into(seed, image, "file");
    assert_eq!(seed.nodes[file].node_type, IMAGE_NODE);
    let source = image_source(&seed.nodes[file].parameters).unwrap().unwrap();
    let ImageFile::Path(path) = source.file() else {
        panic!("a path")
    };
    assert!(
        Path::new(&**path).is_absolute() && path.ends_with("wood.png"),
        "{path}"
    );
    // d times map_d's -imfchan m: the alpha of a color4 image.
    let times = into(seed, phong, "opacity");
    assert_eq!(seed.nodes[times].parameters["in2"], Value::Number(0.75));
    let alpha = into(seed, times, "in1");
    assert_eq!(node(seed, "ND_extract_color4"), [alpha]);
    assert_eq!(seed.nodes[alpha].parameters["index"], Value::Integer(3));
    assert_eq!(node(seed, "ND_image_color4"), [into(seed, alpha, "in")]);
    // The height map shared by map_Bump and disp is one Image node.
    let heights = (seed.nodes.iter())
        .filter(|n| n.name.as_deref() == Some("height"))
        .count();
    assert_eq!(heights, 1);
    let [material] = node(seed, "ND_surfacematerial")[..] else {
        panic!("disp shows a material")
    };
    assert!(seed.nodes[material].display && !seed.nodes[phong].display);
}

#[test]
fn the_pbr_extension_is_a_standard_surface_beside_a_phong_surface_nothing_reads() {
    let (held, _) = read_library("pbr", EVERY, &[]);
    let seed = &held.materials[1].inside;
    let [surface] = node(seed, "ND_standard_surface_surfaceshader")[..] else {
        panic!("one Standard Surface")
    };
    let [phong] = node(seed, "ND_kinogaki_phong_surface_surfaceshader")[..] else {
        panic!("the classic statements beside it")
    };
    assert!(seed.nodes[surface].display);
    assert!(
        !seed.wires.iter().any(|w| w.from == phong),
        "nothing reads it"
    );
    let params = &seed.nodes[surface].parameters;
    assert_eq!(
        (&params["base"], &params["emission"]),
        (&Value::Number(1.0), &Value::Number(1.0))
    );
    assert_eq!(params["specular_IOR"], Value::Number(1.5));
    assert_eq!(seed.nodes[phong].parameters["illum"], Value::Integer(3));
    assert_eq!(
        seed.nodes[phong].parameters["transparency_factor"],
        Value::Number(0.5)
    );
    // d times map_d into a color: a grey of the green channel times 0.5.
    let grey = into(seed, surface, "opacity");
    let times = into(seed, grey, "in");
    assert_eq!(seed.nodes[times].parameters["in2"], Value::Number(0.5));
    let green = into(seed, times, "in1");
    assert_eq!(seed.nodes[green].parameters["index"], Value::Integer(1));
    // The bump map reads the normal map, and drives the normal.
    let bump = into(seed, surface, "normal");
    assert_eq!(seed.nodes[bump].parameters["scale"], Value::Number(0.25));
    let normal = into(seed, bump, "normal");
    assert_eq!(seed.nodes[normal].parameters["scale"], Value::Number(0.8));
}

/// A color service whose working space is twice linear Rec.709, by name.
#[derive(Debug)]
struct Doubled;
impl kinogaki_studio_api::color::ColorService for Doubled {
    fn working(&self) -> &str {
        "doubled"
    }
    fn convert(
        &self,
        from: &str,
        to: &str,
        rgb: &mut [[f64; 3]],
    ) -> Result<(), kinogaki_studio_api::Error> {
        let k = match (from, to) {
            ("lin_rec709_scene", "doubled") => 2.0,
            ("doubled", "lin_rec709_scene") => 0.5,
            _ => return Err(kinogaki_studio_api::Error::invalid("no such space")),
        };
        rgb.iter_mut().flatten().for_each(|c| *c *= k);
        Ok(())
    }
    fn revision(&self) -> u64 {
        0
    }
}

#[test]
fn colors_read_into_the_working_space_and_write_out_of_it() {
    let dir = Scratch::new("colors");
    let library = "newmtl Red\nKd 0.5 0.25 0.125\nKs 0.1 0.2 0.3\nd 0.5\n";
    std::fs::write(dir.join("lib.mtl"), library).unwrap();
    std::fs::write(dir.join("model.obj"), "mtllib lib.mtl\n").unwrap();
    let reading = Reading::new(Purpose::Objects, Units::METERS, &Doubled);
    let path = dir.join("model.obj");
    let read = Obj
        .read(&Input::file(&path), &Obj.options(), &reading)
        .unwrap();
    let data = read.items[0].data.as_ref().unwrap();
    let held = standard::materials().get(data).unwrap().clone();
    let seed = &held.materials[0].inside;
    let [phong] = node(seed, "ND_kinogaki_phong_surface_surfaceshader")[..] else {
        panic!("a Phong Surface")
    };
    let value = |input: &str| seed.nodes[phong].parameters[input].clone();
    assert_eq!(value("diffuse_color"), Value::Color([1.0, 0.5, 0.25]));
    assert_eq!(value("specular_color"), Value::Color([0.2, 0.4, 0.6]));
    assert_eq!(
        value("opacity"),
        Value::Number(0.5),
        "an opacity isn't a color"
    );
    let items = [
        FileItem::top("Materials", standard::materials().wrap(held.clone())),
        FileItem::top("Red", standard::geometry().wrap(square("Red"))),
    ];
    let writing = Writing::new(Units::METERS, &Doubled);
    let writer = Obj.writer().unwrap();
    let out = dir.join("again.obj");
    (writer.write(&out, &items, &writing, &FormatOptions::new())).unwrap();
    let written = std::fs::read_to_string(dir.join("again.mtl")).unwrap();
    assert!(written.contains("Kd 0.5 0.25 0.125\n"), "{written}");
    assert!(written.contains("Ks 0.1 0.2 0.3\n"), "{written}");
}

#[test]
fn tr_alone_is_the_opacity_s_inverse() {
    let (held, _) = read_library("tr", EVERY, &[]);
    let seed = &held.materials[2].inside;
    let [phong] = node(seed, "ND_kinogaki_phong_surface_surfaceshader")[..] else {
        panic!("a Phong Surface")
    };
    let invert = into(seed, phong, "opacity");
    assert_eq!(seed.nodes[invert].parameters["in"], Value::Number(0.6));
}

#[test]
fn bump_maps_read_as_heights_or_as_normal_maps_as_told() {
    let library = "newmtl a\nmap_Bump -bm 0.5 brick_normal.png\n";
    let (held, warnings) = read_library("heights", library, &[]);
    let seed = &held.materials[0].inside;
    assert_eq!(node(seed, "ND_bump_vector3").len(), 1);
    assert!(
        warnings[0].message.contains("its name says a normal map"),
        "{warnings:?}"
    );
    let set = [("normal_maps", Value::Bool(true))];
    let (held, warnings) = read_library("normals", library, &set);
    let seed = &held.materials[0].inside;
    assert!(warnings.is_empty(), "{warnings:?}");
    let [normal] = node(seed, "ND_normalmap_float")[..] else {
        panic!("a normal map")
    };
    assert_eq!(seed.nodes[normal].parameters["scale"], Value::Number(0.5));
    // Written back as MTL's normal map.
    let (written, _) = write(&Scratch::new("normals-written"), "n", &held);
    assert!(written.materials[0].map("norm").is_some());
}

#[test]
fn what_mtl_records_nowhere_is_left_out_and_said() {
    let library = "newmtl a\nKd 1 1 1\nsharpness 60\nmap_Kd -blendu off -cc on -t 0.1 a.png\nmap_Ns -imfchan l r.png\ndecal d.png\nrefl -type sphere sky.png\n";
    let (held, warnings) = read_library("left", library, &[]);
    let [warning] = &warnings[..] else {
        panic!("{warnings:?}")
    };
    for said in [
        "-blendu",
        "-cc",
        "-t",
        "-imfchan l",
        "decal",
        "refl",
        "sharpness",
    ] {
        assert!(
            warning.message.contains(said),
            "{said}: {}",
            warning.message
        );
    }
    let (written, _) = write(&Scratch::new("left-written"), "a", &held);
    let a = &written.materials[0];
    assert!(a.other.is_empty() && a.map("decal").is_none() && a.map("refl").is_none());
    assert!(a.map("map_Kd").unwrap().options.other.is_empty());
}

#[test]
fn textures_embed_when_asked_and_write_beside_the_library() {
    let dir = Scratch::new("embed");
    std::fs::write(dir.join("wood.png"), [1, 2, 3]).unwrap();
    std::fs::write(
        dir.join("lib.mtl"),
        "newmtl a\nmap_Kd wood.png\nmap_Ks gone.png\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("model.obj"),
        "mtllib lib.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    let (held, warnings) = read(&dir.join("model.obj"), &[(EMBED, Value::Bool(true))]);
    assert!(
        (warnings.iter()).any(|w| w.message == "no such files: gone.png"),
        "{warnings:?}"
    );
    let out = Scratch::new("embed-written");
    let (written, _) = write(&out, "scene", &held);
    let a = &written.materials[0];
    assert_eq!(a.map("map_Kd").unwrap().path, "scene_textures/wood.png");
    assert_eq!(
        std::fs::read(out.join("scene_textures/wood.png")).unwrap(),
        [1, 2, 3]
    );
    // A path stays one, relative to where the library is written.
    let gone = Path::new(&a.map("map_Ks").unwrap().path).to_path_buf();
    assert_eq!(
        out.join(&gone).canonicalize().ok(),
        None,
        "it names the file that isn't there"
    );
    assert!(
        gone.ends_with("gone.png") && gone.starts_with(".."),
        "{}",
        gone.display()
    );
}

#[test]
fn another_model_writes_through_standard_surface() {
    use kinogaki_studio_api::network::NewNode;
    use kinogaki_studio_api::standard::ImportedMaterial;
    let mut inside = NetworkSeed::default();
    let open = NewNode::materialx("ND_open_pbr_surface_surfaceshader").unwrap();
    let open = open
        .with("base_color", Value::Color([0.2, 0.4, 0.6]))
        .with("base_metalness", Value::Number(1.0))
        .with("specular_roughness", Value::Number(0.35))
        .displayed();
    inside.node(open);
    let m = ImportedMaterial {
        key: "/Looks/Gold".into(),
        name: "Gold".into(),
        inside,
        standard: None,
        double_sided: true,
    };
    let held = Materials {
        materials: vec![m],
        images: Vec::new(),
    };
    let dir = Scratch::new("translated");
    let (written, warnings) = write(&dir, "gold", &held);
    assert!(warnings.is_empty(), "{warnings:?}");
    let gold: &Material = &written.materials[0];
    assert_eq!(gold.name, "Gold");
    assert_eq!((gold.metallic, gold.roughness), (Some(1.0), Some(0.35)));
    // OpenPBR's base weight 1 is Standard Surface's base 1: Kd as authored.
    assert_eq!(gold.diffuse, Some([0.2, 0.4, 0.6]));
    let text = std::fs::read_to_string(dir.join("gold.obj")).unwrap();
    assert!(text.contains("usemtl Gold"), "named by its name: {text}");
}

/// Every item of the OBJ at `path` (its materials first, then its geometry)
/// read with the defaults: a library written into a scratch folder names
/// the fixtures' textures where they are.
fn items(path: &Path) -> Vec<FileItem> {
    let colors = Unconverted;
    let reading = Reading::new(Purpose::Objects, Units::METERS, &colors);
    let read = Obj
        .read(&Input::file(path), &Obj.options(), &reading)
        .unwrap();
    // Blender writes its normal map under map_Bump, which reads as heights.
    let said = |w: &Diagnostic| w.message.contains("its name says a normal map");
    assert!(read.warnings.iter().all(said), "{:?}", read.warnings);
    read.items
}

/// Blender's exports, read and written back: the same polygons naming the
/// same materials, and the same statements and values.
#[test]
fn blender_exports_write_back_unchanged() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blender");
    for name in ["cube", "cube_pbr"] {
        let read = items(&fixtures.join(format!("{name}.obj")));
        let dir = Scratch::new(&format!("blender-{name}"));
        let file = dir.join(format!("{name}.obj"));
        let colors = Unconverted;
        let writing = Writing::new(Units::METERS, &colors);
        let writer = Obj.writer().expect("it writes");
        let warnings = writer
            .write(&file, &read, &writing, &FormatOptions::new())
            .unwrap();
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
        let mut written = normalized(mtl::read_file(dir.join(format!("{name}.mtl"))).unwrap());
        for m in &mut written.materials {
            for map in &mut m.maps {
                map.path = Path::new(&map.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into();
            }
        }
        let original = mtl::read_file(fixtures.join(format!("{name}.mtl"))).unwrap();
        assert_eq!(written, normalized(original), "{name}");
        let again = items(&file);
        let geometry = |items: &[FileItem]| {
            let data = items[1].data.as_ref().unwrap();
            standard::geometry().get(data).unwrap().clone()
        };
        let (a, b) = (geometry(&read), geometry(&again));
        assert_eq!(
            (a.positions(), a.primitives()),
            (b.positions(), b.primitives()),
            "{name}"
        );
        let material = |g: &Geometry| {
            let text = g
                .attributes()
                .text(Domain::Primitive, names::MATERIAL)
                .unwrap();
            text.iter()
                .map(|t| t.map(str::to_owned))
                .collect::<Vec<_>>()
        };
        assert_eq!(material(&a), material(&b), "{name}");
    }
}

/// PLAN's budget: a thousand materials add at most 5 ms to an import (M4
/// Max, release).
#[test]
#[ignore = "hand: a measurement, meaningful in release"]
fn hand_run_a_thousand_materials_read_within_their_budget() {
    let dir = Scratch::new("thousand");
    let mut library = String::new();
    for n in 0..1000 {
        library += &format!(
            "newmtl m{n}\nKa 1 1 1\nKd 0.8 0.5 0.2\nKs 0.5 0.5 0.5\nNs 250\nNi 1.5\nd 1\nillum 2\nmap_Kd -s 2 2 1 wood{n}.png\nmap_Bump -bm 0.5 bump{n}.png\n\n"
        );
    }
    std::fs::write(dir.join("lib.mtl"), library).unwrap();
    let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    std::fs::write(dir.join("bare.obj"), obj).unwrap();
    std::fs::write(dir.join("model.obj"), format!("mtllib lib.mtl\n{obj}")).unwrap();
    let colors = Unconverted;
    let reading = Reading::new(Purpose::Objects, Units::METERS, &colors);
    let best = |name: &str| {
        let path = dir.join(name);
        let input = Input::file(&path);
        (0..10)
            .map(|_| {
                let started = std::time::Instant::now();
                Obj.read(&input, &Obj.options(), &reading).unwrap();
                started.elapsed().as_secs_f64() * 1000.0
            })
            .fold(f64::INFINITY, f64::min)
    };
    let added = best("model.obj") - best("bare.obj");
    println!("OBJ a thousand materials add {added:.2} ms to a read");
    assert!(added <= 5.0, "{added:.2} ms");
}
