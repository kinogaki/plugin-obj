//! Hostile files: each shape that once took OBJ's reader seconds to
//! minutes, or reached files outside the file's folder, read as File ▸
//! Import reads one (`read_hostile`: a 2 MiB stack and a budget), ending
//! in its items, a clean error or a warning within a time bound.
use kinogaki_obj::Obj;
use kinogaki_studio_api::format::ReadLimits;
use kinogaki_studio_api::format::options::EMBED;
use kinogaki_studio_api::geometry::Domain;
use kinogaki_studio_api::geometry::image::ImageFile;
use kinogaki_studio_api::image_files::image_source;
use kinogaki_studio_api::standard;
use kinogaki_studio_api::{Error, FileFormat, FormatOptions, Imported, Input, Value};
use kinogaki_studio_api_testing::format::Scratch;
use kinogaki_studio_api_testing::read_hostile;
use std::path::Path;
use std::time::Duration;

/// How long a hostile read may take in a debug build: seconds where the
/// shapes once took tens of seconds.
const BOUND: Duration = Duration::from_secs(10);

/// A triangle after `before`, then `faces` more of it.
fn triangles(before: &str, faces: usize) -> String {
    format!(
        "{before}v 0 0 0\nv 1 0 0\nv 0 1 0\n{}",
        "f 1 2 3\n".repeat(faces)
    )
}

/// `text` read as a file held in memory within `limits`.
fn read(text: &str, limits: ReadLimits) -> (Result<Imported, Error>, Duration) {
    let input = Input::memory("hostile.obj", text.as_bytes());
    read_hostile(&Obj, &input, &Obj.options(), limits)
}

/// The file at `path` read with `options` set over the defaults.
fn read_file(path: &Path, options: &[(&str, bool)]) -> Imported {
    let mut chosen: FormatOptions = Obj.options();
    for (name, on) in options {
        chosen.insert((*name).into(), Value::Bool(*on));
    }
    let (read, took) = read_hostile(&Obj, &Input::file(path), &chosen, ReadLimits::default());
    assert!(took < BOUND, "{took:?}");
    read.unwrap()
}

/// A scratch folder holding `scene/` (where the file is read).
fn scratch(name: &str) -> Scratch {
    let dir = Scratch::new(&format!("obj-hostile-{name}"));
    std::fs::create_dir_all(dir.join("scene")).unwrap();
    dir
}

#[test]
fn a_face_in_twenty_thousand_groups_reads_in_linear_time() {
    // 209 KB once took 17 s and 1.3 GB: every face was pushed into each
    // group its `g` line names, the line split again for every face.
    let names: Vec<String> = (0..20_000).map(|n| format!("g{n}")).collect();
    let text = triangles(&format!("g {}\n", names.join(" ")), 10_000);
    assert!(text.len() > 200_000);
    let (whole, took) = read(&text, ReadLimits::default());
    let whole = whole.unwrap();
    assert!(took < BOUND, "{took:?}");
    let geometry = standard::geometry()
        .get(whole.items[0].data.as_ref().unwrap())
        .unwrap();
    let group = geometry
        .attributes()
        .group(Domain::Primitive, "g19999")
        .unwrap();
    assert_eq!(group.len(), 10_000);
    // Within a small budget, the members are refused rather than made.
    let small = ReadLimits {
        total: 8 << 20,
        ..ReadLimits::default()
    };
    let (read, took) = read(&text, small);
    assert!(
        read.unwrap_err()
            .message()
            .contains("over the read's budget")
    );
    assert!(took < BOUND);
}

#[test]
fn unknown_keywords_read_in_linear_time_and_are_said_briefly() {
    // 1.5 MB once took 28 s: each keyword was looked for among the others,
    // and its line counted from the start.
    let keywords: String = (0..150_000).map(|n| format!("kw{n} x\n")).collect();
    let text = triangles(&keywords, 1);
    assert!(text.len() > 1_400_000);
    let (whole, took) = read(&text, ReadLimits::default());
    let whole = whole.unwrap();
    assert!(took < BOUND, "{took:?}");
    let said = &whole.warnings[0].message;
    assert!(said.ends_with("149984 other kinds of statement"), "{said}");
    assert!(said.len() < 1000);
}

#[test]
fn a_polygon_of_three_hundred_thousand_corners_is_checked_in_n_log_n() {
    let corners: Vec<String> = (1..=300_000).map(|n| n.to_string()).collect();
    let text = format!("{}f {}\n", "v 0 0 0\n".repeat(300_000), corners.join(" "));
    let (read, took) = read(&text, ReadLimits::default());
    assert!(read.is_ok() && took < BOUND, "{took:?}");
}

#[test]
fn tables_past_the_budget_are_refused_before_they_are_made() {
    let text = "v 0 0 0\n".repeat(100_000);
    let small = ReadLimits {
        total: 1 << 20,
        ..ReadLimits::default()
    };
    let (read, took) = read(&text, small);
    assert!(
        read.unwrap_err()
            .message()
            .contains("over the read's budget")
    );
    assert!(took < BOUND);
}

#[test]
fn libraries_and_maps_out_of_the_folder_are_read() {
    let dir = scratch("reach");
    std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
    std::fs::write(
        dir.join("elsewhere/far.mtl"),
        "newmtl far\nmap_Kd far.png\n",
    )
    .unwrap();
    std::fs::write(dir.join("elsewhere/far.png"), b"far").unwrap();
    let absolute = dir.join("elsewhere/far.png").display().to_string();
    std::fs::write(
        dir.join("scene/lib.mtl"),
        format!("newmtl near\nmap_Kd {absolute}\nmap_Ks ../elsewhere/far.png\nmap_Ns wood.png\n"),
    )
    .unwrap();
    std::fs::write(dir.join("scene/wood.png"), b"wood").unwrap();
    let obj = dir.join("scene/model.obj");
    std::fs::write(
        &obj,
        triangles("mtllib lib.mtl\nmtllib ../elsewhere/far.mtl\n", 1),
    )
    .unwrap();
    let read = read_file(&obj, &[(EMBED, true)]);
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    let held = standard::materials()
        .get(read.items[0].data.as_ref().unwrap())
        .unwrap();
    let keys: Vec<&str> = held.materials.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["near", "far"], "far's library is read where it is");
    // Each map embedded, found by its absolute path or a parent folder.
    let embedded = |m: usize| -> Vec<Vec<u8>> {
        let nodes = &held.materials[m].inside.nodes;
        let sources = nodes
            .iter()
            .filter_map(|n| image_source(&n.parameters).ok().flatten());
        let bytes = sources.filter_map(|s| match s.file() {
            ImageFile::Embedded(e) => Some(e.bytes().to_vec()),
            ImageFile::Path(_) => None,
        });
        let mut bytes: Vec<Vec<u8>> = bytes.collect();
        bytes.sort();
        bytes.dedup();
        bytes
    };
    assert_eq!(embedded(0), [b"far".to_vec(), b"wood".to_vec()]);
    assert_eq!(embedded(1), [b"far".to_vec()]);
}

#[test]
fn a_map_that_is_a_pipe_is_never_opened() {
    let dir = scratch("pipe");
    let pipe = dir.join("scene/pipe.png");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(dir.join("scene/lib.mtl"), "newmtl a\nmap_Kd pipe.png\n").unwrap();
    let obj = dir.join("scene/model.obj");
    std::fs::write(&obj, triangles("mtllib lib.mtl\n", 1)).unwrap();
    let read = read_file(&obj, &[(EMBED, true)]);
    assert!(
        (read.warnings.iter()).any(|w| w.message == "not regular files, never read: pipe.png"),
        "{:?}",
        read.warnings
    );
}

#[test]
fn a_library_named_five_hundred_times_is_read_once() {
    // 500 `mtllib` lines once read their library 500 times, each material
    // checked against every one before it.
    let dir = scratch("repeat");
    let library: String = (0..5_000)
        .map(|n| format!("newmtl m{n}\nKd 1 0 0\n"))
        .collect();
    std::fs::write(dir.join("scene/lib.mtl"), library).unwrap();
    let obj = dir.join("scene/model.obj");
    std::fs::write(&obj, triangles(&"mtllib lib.mtl\n".repeat(500), 1)).unwrap();
    let read = read_file(&obj, &[]);
    let held = standard::materials()
        .get(read.items[0].data.as_ref().unwrap())
        .unwrap();
    assert_eq!(held.materials.len(), 5_000);
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
}

#[test]
fn a_file_held_in_memory_reads_no_library() {
    let (whole, _) = read(&triangles("mtllib lib.mtl\n", 1), ReadLimits::default());
    let read = whole.unwrap();
    assert!(
        (read.warnings.iter()).any(|w| w.message.starts_with("hostile.obj is held in memory")),
        "{:?}",
        read.warnings
    );
}
