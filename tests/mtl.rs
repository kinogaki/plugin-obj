//! Material libraries: statements, texture options, round trips, errors.
use kinogaki_obj::obj::{
    MalformedKind,
    mtl::{self, MapOptions, Material, MaterialLibrary, TextureMap},
};
use kinogaki_studio_api_testing::format::Scratch;

const SAMPLE: &str = "# exported\nKd 9 9 9\nnewmtl steel  \n\tKa 0.1 0.1 0.1\nKd 0.5 0.6 0.7 # grey\nKs 1\nNs 250\nNi 1.45\nd 0.9\nillum 2\nmap_Kd -s 2 2 1 -o 0.5 0.5 textures/brushed steel.png\nmap_Bump -bm 0.3 normal.png\nbump plain.png\nPr 0.25\nPm 1\nKe xyz 0.1 0.2 0.3\nsharpness 60\n\nnewmtl glass\nTr 0.75\nTf 0.9 1 0.9\nnorm -clamp on n.png\nmap_d -imfchan m -mm 0.1 0.8 -blendu off -t 0.5 a.png\n";

#[test]
fn reads_every_statement() {
    let library = mtl::parse(SAMPLE.as_bytes()).unwrap();
    assert_eq!(library.materials.len(), 2);
    let steel = library.get("steel").unwrap();
    assert_eq!(steel.ambient, Some([0.1; 3]));
    assert_eq!(steel.diffuse, Some([0.5, 0.6, 0.7]));
    assert_eq!(steel.specular, Some([1.0; 3]));
    assert_eq!(steel.shininess, Some(250.0));
    assert_eq!(steel.optical_density, Some(1.45));
    assert_eq!(steel.dissolve, Some(0.9));
    assert_eq!(steel.illumination, Some(2));
    assert_eq!(steel.roughness, Some(0.25));
    assert_eq!(steel.metallic, Some(1.0));
    assert_eq!(
        steel.map("map_kd").unwrap(),
        &TextureMap {
            kind: "map_Kd".into(),
            options: MapOptions {
                scale: Some([2.0, 2.0, 1.0]),
                offset: Some([0.5, 0.5, 0.0]),
                ..MapOptions::default()
            },
            path: "textures/brushed steel.png".into()
        }
    );
    assert_eq!(steel.map("map_Bump").unwrap().options.bump, Some(0.3));
    assert_eq!(steel.map("bump").unwrap().path, "plain.png");
    assert_eq!(
        steel.other,
        vec![
            ("Ke".to_string(), "xyz 0.1 0.2 0.3".to_string()),
            ("sharpness".to_string(), "60".to_string())
        ]
    );
    let glass = library.get("glass").unwrap();
    assert_eq!((glass.dissolve, glass.transparency), (None, Some(0.75)));
    assert_eq!(glass.transmission_filter, Some([0.9, 1.0, 0.9]));
    assert_eq!(glass.map("norm").unwrap().options.clamp, Some(true));
    let alpha = &glass.map("map_d").unwrap().options;
    assert_eq!((alpha.channel, alpha.range), (Some('m'), Some([0.1, 0.8])));
    let other = |name: &str, values: &[&str]| {
        (
            name.to_owned(),
            values.iter().map(|v| v.to_string()).collect(),
        )
    };
    assert_eq!(
        alpha.other,
        [other("-blendu", &["off"]), other("-t", &["0.5"])]
    );
}

#[test]
fn round_trips() {
    let library = mtl::parse(SAMPLE.as_bytes()).unwrap();
    let again = mtl::parse(&mtl::to_bytes(&library)).unwrap();
    assert_eq!(again, library);
    let built = MaterialLibrary {
        materials: vec![Material {
            name: "a".into(),
            diffuse: Some([1.0 / 3.0, 0.1 + 0.2, 1e-300]),
            clearcoat: Some(0.5),
            anisotropy_rotation: Some(0.125),
            ..Material::default()
        }],
    };
    assert_eq!(mtl::parse(&mtl::to_bytes(&built)).unwrap(), built);
    let directory = Scratch::new("obj-mtl");
    let path = directory.join("scene.mtl");
    mtl::write_file(&path, &library).unwrap();
    assert_eq!(mtl::read_file(&path).unwrap(), library);
}

#[test]
fn errors_name_their_line() {
    for (text, line) in [
        ("newmtl a\nKd 1 x 1\n", 2),
        ("newmtl a\n\nNs\n", 3),
        ("newmtl a\nillum two\n", 2),
        ("newmtl a\nmap_Kd -s 1\n", 2),
        ("newmtl a\nKd 1 1\n", 2),
        ("newmtl a\nmap_Kd -mm 1 a.png\n", 2),
    ] {
        let error = mtl::parse(text.as_bytes()).unwrap_err();
        let error = error.malformed().unwrap();
        assert_eq!(error.line, line, "{text:?}: {error}");
        assert_ne!(error.kind, MalformedKind::Limit);
    }
}

#[test]
fn library_paths_try_the_whole_value_first() {
    assert_eq!(mtl::library_paths("scene.mtl"), vec!["scene.mtl"]);
    assert_eq!(
        mtl::library_paths("my scene.mtl"),
        vec!["my scene.mtl", "my", "scene.mtl"]
    );
}
