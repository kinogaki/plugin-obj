//! Writing: exact round trips of every part of the model, validation and
//! atomic file replacement.
#[path = "common/corpus.rs"]
mod corpus;
#[path = "common/files.rs"]
mod files;
#[path = "common/random.rs"]
mod random;
use kinogaki_obj::obj::{
    Elements, NONE, Obj, Run, WriteOptions, parse, to_bytes, write_file, write_with,
};
use kinogaki_studio_api_testing::format::Scratch;
use kinogaki_studio_api_testing::workers;
use random::Random;

fn round_trip(obj: &Obj) -> Obj {
    let text = to_bytes(obj).unwrap();
    let again =
        parse(&text).unwrap_or_else(|error| panic!("{error}\n{}", String::from_utf8_lossy(&text)));
    assert_eq!(&again, obj, "{}", String::from_utf8_lossy(&text));
    again
}

fn elements(rows: &[&[u32]], texcoords: Option<&[u32]>, normals: Option<&[u32]>) -> Elements {
    let mut elements = Elements::default();
    for row in rows {
        elements.positions.extend_from_slice(row);
        elements.offsets.push(elements.positions.len() as u32);
    }
    elements.texcoords = texcoords.map(<[u32]>::to_vec).unwrap_or_default();
    elements.normals = normals.map(<[u32]>::to_vec).unwrap_or_default();
    elements
}

fn run<T>(face: u32, line: u32, point: u32, value: T) -> Run<T> {
    Run {
        face,
        line,
        point,
        value,
    }
}

#[test]
fn every_part_of_the_model_round_trips() {
    let obj = Obj {
        positions: vec![
            [0.1 + 0.2, 1.0 / 3.0, -1e-300],
            [f64::from_bits(0x412e847fffffffff), 2.0 / 7.0, 3.0],
            [4.0, -0.0, 6.0],
            [5e-324, 1e300, -1.5],
        ],
        weights: vec![1.0, 2.0, 1.0, 0.5],
        colors: vec![[1.0; 3], [0.25, 0.5, 0.75], [0.0; 3], [1.0, 0.0, 0.0]],
        texcoords: vec![[0.0, 0.0, 0.0], [1.0, 0.5, 0.0], [0.25, 0.75, 0.125]],
        normals: vec![[0.0, 0.0, 1.0], [0.6, 0.0, 0.8]],
        faces: elements(
            &[&[0, 1, 2], &[1, 3, 2, 0]],
            Some(&[0, 1, 2, NONE, 1, 1, 0]),
            Some(&[0, NONE, 1, 1, 1, 0, 0]),
        ),
        lines: elements(&[&[0, 1], &[1, 2, 3]], Some(&[NONE, 0, 1, 2, NONE]), None),
        points: elements(&[&[3], &[0, 2]], None, None),
        objects: vec![
            run(0, 0, 0, "first piece".into()),
            run(1, 1, 0, "second".into()),
        ],
        groups: vec![run(0, 0, 0, "a b".into()), run(2, 2, 1, String::new())],
        materials: vec![run(1, 0, 0, "steel".into())],
        smoothing: vec![run(0, 0, 0, 1), run(1, 2, 0, 0), run(2, 2, 2, 7)],
        material_libraries: vec!["scene.mtl".into(), "with space.mtl".into()],
        skipped: Vec::new(),
    };
    obj.validate().unwrap();
    round_trip(&obj);
}

#[test]
fn luce_obj_round_trip_cases() {
    // Polygon sizes, the cube, a 1000-corner star and textured faces.
    for text in [
        &files::star()[..],
        b"v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n",
        b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 2 0 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\nvn 0.6 0 0.8\nf 1/1/1 2/2/1 3/3/2 4/4/1\nf 2/2/2 5/1/1 3/3/1\n",
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1//1 2//1 3//1\n",
    ] {
        round_trip(&parse(text).unwrap());
    }
    let normals_only = parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1//1 2//1 3//1\n").unwrap();
    assert!(
        String::from_utf8(to_bytes(&normals_only).unwrap())
            .unwrap()
            .contains("f 1//1 2//1 3//1")
    );
}

#[test]
fn random_models_round_trip_exactly() {
    for seed in 1..=20 {
        let mut random = Random::new(seed);
        let count = 1 + random.below(50) as usize;
        let value = |random: &mut Random| {
            let bits = random.next();
            let value = f64::from_bits(bits);
            if value.is_finite() {
                value
            } else {
                random.unit() - 0.5
            }
        };
        let mut obj = Obj {
            positions: (0..count)
                .map(|_| [value(&mut random), value(&mut random), value(&mut random)])
                .collect(),
            texcoords: (0..count)
                .map(|_| [value(&mut random), value(&mut random), 0.0])
                .collect(),
            normals: (0..count)
                .map(|_| [value(&mut random), value(&mut random), value(&mut random)])
                .collect(),
            ..Obj::default()
        };
        for _ in 0..random.below(40) {
            let size = 3 + random.below(5) as usize;
            for _ in 0..size {
                obj.faces.positions.push(random.below(count as u64) as u32);
            }
            obj.faces.offsets.push(obj.faces.positions.len() as u32);
        }
        let corners = obj.faces.positions.len();
        obj.faces.texcoords = (0..corners)
            .map(|_| random.below(count as u64) as u32)
            .collect();
        obj.faces.normals = (0..corners)
            .map(|_| random.below(count as u64) as u32)
            .collect();
        round_trip(&obj);
    }
}

#[test]
fn reading_then_writing_keeps_the_corpus() {
    for (name, text) in corpus::corpus() {
        let obj = parse(&text).unwrap();
        let again = parse(&to_bytes(&obj).unwrap()).unwrap();
        assert_eq!(again, obj, "{name}");
    }
}

#[test]
fn invalid_models_are_refused() {
    let base = parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let mut cases = Vec::new();
    let mut obj = base.clone();
    obj.faces.positions[2] = 9;
    cases.push(obj);
    let mut obj = base.clone();
    obj.faces.offsets = vec![0, 2, 3];
    cases.push(obj);
    let mut obj = base.clone();
    obj.positions[0][1] = f64::NAN;
    cases.push(obj);
    let mut obj = base.clone();
    obj.weights = vec![1.0];
    cases.push(obj);
    let mut obj = base.clone();
    obj.objects.push(run(0, 0, 0, "two\nlines".into()));
    cases.push(obj);
    let mut obj = base.clone();
    obj.groups.push(run(2, 0, 0, "past the end".into()));
    cases.push(obj);
    let mut obj = base;
    obj.faces.texcoords = vec![NONE, 0];
    cases.push(obj);
    for obj in cases {
        assert!(obj.validate().is_err());
        assert!(to_bytes(&obj).is_err());
    }
}

#[test]
fn files_are_replaced_atomically_with_a_comment() {
    let directory = Scratch::new("obj-write");
    let path = directory.join("out.obj");
    std::fs::write(&path, b"old").unwrap();
    let obj = parse(&files::chunked(60)).unwrap();
    workers::install(3);
    let options = WriteOptions {
        comment: Some("Kinogaki Studio\nY up".into()),
    };
    write_file(&path, &obj, &options).unwrap();
    let text = std::fs::read(&path).unwrap();
    assert!(text.starts_with(b"# Kinogaki Studio\n# Y up\n"));
    assert_eq!(parse(&text).unwrap(), obj);
    assert_eq!(
        std::fs::read_dir(&directory).unwrap().count(),
        1,
        "no temporary file is left"
    );
    // Thread counts do not change the text.
    workers::install(1);
    let mut one = Vec::new();
    write_with(&obj, &mut one, &options).unwrap();
    assert_eq!(one, text);
    // A failed write leaves the old file.
    let mut invalid = obj.clone();
    invalid.faces.positions[0] = u32::MAX - 1;
    assert!(write_file(&path, &invalid, &options).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), text);
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
}
