//! The obj module against luce-obj (the Luce package it replaces) on the corpus:
//! every file must give luce-obj's mesh exactly (in luce's f32 precision),
//! and the same `Obj` however it is split across threads, chunks and blocks.
//! tools/luce-oracle/refresh.sh regenerates the expected fingerprints.
#[path = "common/corpus.rs"]
mod corpus;
#[path = "common/files.rs"]
mod files;
#[path = "common/fingerprint.rs"]
mod fingerprint;
#[path = "common/random.rs"]
mod random;
use fingerprint::hash;
use kinogaki_obj::obj::{Obj, ReadOptions, parse_with, read};

fn expected() -> Vec<(String, String)> {
    String::from_utf8(files::fixture("luce-oracle.txt"))
        .unwrap()
        .lines()
        .map(|line| {
            let (name, value) = line.split_once('\t').unwrap();
            (name.to_string(), value.to_string())
        })
        .collect()
}

/// The same text read whole, in many small chunks on many threads, and
/// streamed in small blocks; all must agree.
fn read_every_way(text: &[u8]) -> Obj {
    kinogaki_studio_api_testing::workers::install(8);
    let whole = parse_with(text, ReadOptions::default()).unwrap();
    let chunked = parse_with(
        text,
        ReadOptions {
            chunk_bytes: 4096,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(chunked, whole);
    let streamed = read(
        text,
        ReadOptions {
            chunk_bytes: 1024,
            block_bytes: 20_000,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(streamed, whole);
    whole
}

#[test]
fn matches_luce_obj_on_the_corpus() {
    let expected = expected();
    let corpus = corpus::corpus();
    assert_eq!(expected.len(), corpus.len(), "refresh luce-oracle.txt");
    for ((name, text), (expected_name, want)) in corpus.iter().zip(&expected) {
        assert_eq!(name, expected_name);
        assert!(
            !want.starts_with("ERROR"),
            "{name}: luce-obj failed: {want}"
        );
        let obj = read_every_way(text);
        assert_eq!(&fingerprint(&obj), want, "{name}");
    }
}

/// The checked-in fingerprints of luce-obj's output match the oracle's
/// fingerprint function applied to its own canonical form: a write and read
/// of our model gives the same mesh.
#[test]
fn writing_keeps_the_luce_mesh() {
    for (name, text) in corpus::corpus() {
        let obj = kinogaki_obj::obj::parse(&text).unwrap();
        let written = kinogaki_obj::obj::to_bytes(&obj).unwrap();
        let again = kinogaki_obj::obj::parse(&written).unwrap();
        assert_eq!(again, obj, "{name}");
        assert_eq!(fingerprint(&again), fingerprint(&obj), "{name}");
    }
}

/// The mesh luce-obj builds from `obj`, in its precision (f32): positions
/// divided by their weights, polygons, corner `uv` (u, v) and `N` when any
/// face references them (0 where a corner does not), as a short fingerprint.
fn fingerprint(obj: &Obj) -> String {
    hash(obj, &luce_points(obj))
}

/// Points as luce-geocore stores them: f32 offsets from an f64 origin, which
/// is the bounds center when the model is far from the world origin
/// (reach > 4096 or > 8 × its size), else zero.
fn luce_points(obj: &Obj) -> Vec<[f64; 3]> {
    let points: Vec<[f64; 3]> = (0..obj.positions.len()).map(|i| obj.position(i)).collect();
    let Some(first) = points.first() else {
        return points;
    };
    let (mut low, mut high) = (*first, *first);
    for point in &points {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    let size = [high[0] - low[0], high[1] - low[1], high[2] - low[2]];
    let reach = (0..3)
        .map(|a| low[a].abs().max(high[a].abs()))
        .fold(0.0, f64::max);
    let span = size[0].max(size[1]).max(size[2]);
    let origin = if reach > 4096.0 || reach > span * 8.0 {
        [
            low[0] + size[0] * 0.5,
            low[1] + size[1] * 0.5,
            low[2] + size[2] * 0.5,
        ]
    } else {
        [0.0; 3]
    };
    points
        .iter()
        .map(|p| std::array::from_fn(|a| f64::from((p[a] - origin[a]) as f32) + origin[a]))
        .collect()
}
