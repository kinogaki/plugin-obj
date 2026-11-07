//! Reading: records, references, statements, errors with positions, limits.
#[path = "common/files.rs"]
mod files;
use kinogaki_obj::obj::{
    Limits, MalformedKind, NONE, Obj, ObjError, ReadOptions, Run, parse, parse_with, read_file_with,
};

fn malformed(text: &str) -> kinogaki_obj::obj::Malformed {
    match parse(text.as_bytes()) {
        Err(ObjError::Malformed(error)) => error,
        other => panic!("{text:?} should fail, got {other:?}"),
    }
}

#[test]
fn indices_uvs_and_normals() {
    let obj = parse(b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf -4/1 -3/2 -2/3 -1/4\n").unwrap();
    assert_eq!(obj.positions.len(), 4);
    assert_eq!(obj.faces.len(), 1);
    assert_eq!(obj.faces.corners(0), &[0, 1, 2, 3]);
    assert_eq!(obj.faces.texcoords, vec![0, 1, 2, 3]);
    assert!(obj.faces.normals.is_empty());
    assert_eq!(obj.texcoords[2], [1.0, 1.0, 0.0]);

    let obj = parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nvt 0.5 0.25 0.125\nf 1//1 2/1 3/1/1\n")
        .unwrap();
    assert_eq!(obj.faces.texcoords, vec![NONE, 0, 0]);
    assert_eq!(obj.faces.normals, vec![0, NONE, 0]);
    assert_eq!(obj.texcoords[0], [0.5, 0.25, 0.125]);
}

#[test]
fn the_65536_corner_regression() {
    let obj = parse(&files::large()).unwrap();
    assert_eq!(obj.faces.len(), 22000);
    assert_eq!(obj.faces.corner_count(), 66000);
    assert_eq!(obj.normals[obj.faces.normals[65999] as usize][2], 1.0);
}

#[test]
fn crlf_tabs_and_blank_runs_read_alike() {
    let mixed = parse(&files::fixture("mixed.obj")).unwrap();
    let crlf = parse(&files::mixed_crlf()).unwrap();
    assert_eq!(mixed, crlf);
    assert_eq!(mixed.material_libraries, vec!["scene.mtl".to_string()]);
}

#[test]
fn comments_end_lines_anywhere() {
    let obj =
        parse(b"# header\n  v\t0 0 0 # a point\n v 1 0 0\nv 0 1 0#glued\nf 1 2 3 # the face\n#end")
            .unwrap_err();
    // `0#glued` is not a number: a comment must start a token.
    assert_eq!(obj.malformed().unwrap().line, 4);
    let obj = parse(b"v 0 0 0 # a\nv 1 0 0\nv 0 1 0\nvt 0.5 # one value\nf 1/1 2/1 3/1 # face\n")
        .unwrap();
    assert_eq!(obj.texcoords, vec![[0.5, 0.0, 0.0]]);
}

#[test]
fn n_gons_of_any_size() {
    let star = parse(&files::star()).unwrap();
    assert_eq!(star.faces.len(), 1);
    assert_eq!(star.faces.corners(0).len(), 1000);
    let polygons = parse(&files::fixture("polygons.obj")).unwrap();
    assert_eq!(polygons.faces.corners(polygons.faces.len() - 1).len(), 1000);
}

#[test]
fn statements_become_runs() {
    let text = b"mtllib a b.mtl\nmtllib c.mtl\no first\nv 0 0 0\nv 1 0 0\nv 0 1 0\ng\tleft  right \nusemtl red\ns 1\nf 1 2 3\nl 1 2 3\ns off\np 1 2\no second # named\nf 1 2 3\n";
    let obj = parse(text).unwrap();
    assert_eq!(
        obj.material_libraries,
        vec!["a b.mtl".to_string(), "c.mtl".to_string()]
    );
    let run = |face, line, point, value: &str| Run {
        face,
        line,
        point,
        value: value.to_string(),
    };
    assert_eq!(
        obj.objects,
        vec![run(0, 0, 0, "first"), run(1, 1, 1, "second")]
    );
    assert_eq!(obj.groups, vec![run(0, 0, 0, "left right")]);
    assert_eq!(obj.materials, vec![run(0, 0, 0, "red")]);
    assert_eq!(
        obj.smoothing
            .iter()
            .map(|r| (r.face, r.line, r.value))
            .collect::<Vec<_>>(),
        vec![(0, 0, 1), (1, 1, 0)]
    );
    assert_eq!(Obj::face_run(&obj.objects, 0).unwrap(), "first");
    assert_eq!(Obj::face_run(&obj.objects, 1).unwrap(), "second");
    assert_eq!(Obj::face_run(&obj.smoothing, 1), Some(&0));
}

#[test]
fn polylines_and_point_sets() {
    let obj =
        parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nl 1 2 3\nl 1/1 -1/1\np 1 2 3\np -1\n").unwrap();
    assert_eq!(obj.lines.len(), 2);
    assert_eq!(obj.lines.corners(1), &[0, 2]);
    assert_eq!(obj.lines.texcoords, vec![NONE, NONE, NONE, 0, 0]);
    assert_eq!(obj.points.len(), 2);
    assert_eq!(obj.points.corners(1), &[2]);
    assert!(obj.points.texcoords.is_empty());
    let plain = parse(b"v 0 0 0\nv 1 0 0\nl 1 2\n").unwrap();
    assert!(plain.lines.texcoords.is_empty());
}

#[test]
fn weights_and_colors_fill_in_defaults() {
    let obj = parse(b"v 0 0 0\nv 2 0 0 2\nv 0 1 0 1 0.5 0\nv 0 0 1 4 0 0 1\n").unwrap();
    assert_eq!(obj.weights, vec![1.0, 2.0, 1.0, 4.0]);
    assert_eq!(
        obj.colors,
        vec![[1.0; 3], [1.0; 3], [1.0, 0.5, 0.0], [0.0, 0.0, 1.0]]
    );
    assert_eq!(obj.position(1), [1.0, 0.0, 0.0]);
    assert_eq!(obj.position(3), [0.0, 0.0, 0.25]);
    let plain = parse(b"v 0 0 0\n").unwrap();
    assert!(plain.weights.is_empty() && plain.colors.is_empty());
}

#[test]
fn unrepresented_statements_are_skipped_and_reported() {
    let obj = parse(b"v 0 0 0\nvp 0.5\ncstype bspline\ncurv 0 1 1\nvp 0.25\nfoo bar\n").unwrap();
    let skipped: Vec<_> = obj
        .skipped
        .iter()
        .map(|s| (s.keyword.as_str(), s.count, s.first_line))
        .collect();
    assert_eq!(
        skipped,
        vec![
            ("vp", 2, 2),
            ("cstype", 1, 3),
            ("curv", 1, 4),
            ("foo", 1, 6)
        ]
    );
}

#[test]
fn names_and_comments_need_not_be_utf8() {
    let obj = parse(&files::fixture("bad_utf8.obj")).unwrap();
    assert_eq!(obj.faces.len(), 1);
    let obj = parse(b"o caf\xe9\nv 0 0 0\n").unwrap();
    assert_eq!(obj.objects[0].value, "caf\u{fffd}");
    let accents = parse(&files::fixture("accents.obj")).unwrap();
    assert_eq!(accents.objects[0].value, "pièce");
}

#[test]
fn empty_inputs_are_empty_models() {
    for text in ["", "# nothing\n", "\n\n", "o only\n"] {
        let obj = parse(text.as_bytes()).unwrap();
        assert!(obj.is_empty(), "{text:?}");
    }
}

#[test]
fn malformed_records_fail_with_their_position() {
    use MalformedKind::*;
    let cases: &[(&str, MalformedKind, u64, u64)] = &[
        ("v 0 0 0\nf 0 1 2\n", Index, 2, 3),
        ("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n", Index, 4, 7),
        ("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2\n", Syntax, 4, 1),
        ("v 0 0\n", Syntax, 1, 6),
        ("v 0 0 0 0\n", Number, 1, 8),
        ("v 0 0 0 1 1\n", Syntax, 1, 12),
        ("v 1 2 3x\n", Number, 1, 7),
        ("v 1e999 0 0\n", Number, 1, 3),
        ("vn 0 1\n", Syntax, 1, 7),
        ("vn 0 1 0 1\n", Syntax, 1, 10),
        ("vt 0 1 0 1\n", Syntax, 1, 10),
        ("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1/1 2/1 3/1\n", Index, 4, 5),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1// 2// 3//\n",
            Syntax,
            5,
            6,
        ),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 99999999999999999999\n",
            Syntax,
            4,
            7,
        ),
        ("v nan 0 0\n", Number, 1, 3),
        ("v x 0 0\nf 1 2\n", Number, 1, 3),
        ("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3 -4\n", Index, 4, 9),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\nf 1/1/1/1 2 3\n",
            Syntax,
            6,
            7,
        ),
        ("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3x\n", Syntax, 4, 7),
        ("l 1 2\n", Index, 1, 3),
        ("v 0 0 0\nl 1\n", Syntax, 2, 1),
        ("v 0 0 0\np\n", Syntax, 2, 1),
        ("s maybe\n", Syntax, 1, 3),
        ("v 0 0 0 \\\n 1\n", Unsupported, 1, 9),
    ];
    for &(text, kind, line, column) in cases {
        let error = malformed(text);
        assert_eq!(
            (error.kind, error.line, error.column),
            (kind, line, column),
            "{text:?}: {error}"
        );
    }
    // A backslash inside a comment is not a continuation.
    parse(b"v 0 0 0 # C:\\\n").unwrap();
}

#[test]
fn errors_are_the_first_in_file_order_on_any_thread_count() {
    let mut text = files::chunked(160);
    let lines = text.iter().filter(|&&b| b == b'\n').count() as u64;
    text.extend_from_slice(b"f 1 2 999999\n");
    for (threads, chunk_bytes) in [(1, 256 << 10), (8, 4096), (16, 512)] {
        let options = ReadOptions {
            threads,
            chunk_bytes,
            ..ReadOptions::default()
        };
        let error = parse_with(&text, options).unwrap_err();
        let error = error.malformed().unwrap();
        assert_eq!(
            (error.kind, error.line, error.column),
            (MalformedKind::Index, lines + 1, 7)
        );
    }
    // An early error wins over a later one in another chunk.
    let mut text = b"v 0 0 0\nv x 0 0\n".to_vec();
    text.extend_from_slice(&files::chunked(100));
    text.extend_from_slice(b"f 0 0 0\n");
    let options = ReadOptions {
        threads: 8,
        chunk_bytes: 1024,
        ..ReadOptions::default()
    };
    assert_eq!(
        parse_with(&text, options)
            .unwrap_err()
            .malformed()
            .unwrap()
            .line,
        2
    );
}

#[test]
fn limits_bound_every_table() {
    let text = files::chunked(40);
    let read = |limits: Limits| {
        parse_with(
            &text,
            ReadOptions {
                limits,
                ..ReadOptions::default()
            },
        )
    };
    let base = Limits::default();
    assert!(read(base).is_ok());
    for limits in [
        Limits {
            input_bytes: 1000,
            ..base
        },
        Limits {
            vertices: 1000,
            ..base
        },
        Limits {
            elements: 1000,
            ..base
        },
        Limits {
            corners: 1000,
            ..base
        },
        Limits {
            line_bytes: 10,
            ..base
        },
    ] {
        let error = read(limits).unwrap_err();
        assert_eq!(
            error.malformed().unwrap().kind,
            MalformedKind::Limit,
            "{limits:?}: {error}"
        );
    }
}

#[test]
fn streamed_files_read_like_text_in_memory() {
    let directory = std::env::temp_dir().join(format!("obj-read-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("chunked.obj");
    let text = files::chunked(120);
    std::fs::write(&path, &text).unwrap();
    let whole = parse(&text).unwrap();
    for block_bytes in [1 << 30, 100_000, 777, 1] {
        let options = ReadOptions {
            block_bytes,
            chunk_bytes: 2048,
            ..ReadOptions::default()
        };
        assert_eq!(
            read_file_with(&path, options).unwrap(),
            whole,
            "{block_bytes}"
        );
    }
    // Errors keep their line numbers across blocks.
    let mut bad = text.clone();
    bad.extend_from_slice(b"v 1 2\n");
    std::fs::write(&path, &bad).unwrap();
    let lines = text.iter().filter(|&&b| b == b'\n').count() as u64;
    let options = ReadOptions {
        block_bytes: 5000,
        ..ReadOptions::default()
    };
    let error = read_file_with(&path, options).unwrap_err();
    assert_eq!(error.malformed().unwrap().line, lines + 1);
    assert!(matches!(
        read_file_with(directory.join("missing.obj"), options),
        Err(ObjError::Io(_))
    ));
    std::fs::remove_dir_all(&directory).unwrap();
}
