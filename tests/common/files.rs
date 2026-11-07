//! Test inputs from files and generators: the luce-obj fixtures and the files
//! luce-obj's test program generates.

use std::fmt::Write;
use std::path::PathBuf;

pub fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// luce-obj's main.luc fixture: the 65,536-corner buffer regression.
pub fn large() -> Vec<u8> {
    let mut text = String::from("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\n");
    text.push_str(&"f 1/1/1 2/1/1 3/1/1\n".repeat(22000));
    text.into_bytes()
}

/// luce-obj's main.luc fixture: mixed.obj with CRLF line ends and tab/blank runs.
pub fn mixed_crlf() -> Vec<u8> {
    let text = String::from_utf8(fixture("mixed.obj")).unwrap();
    text.replace('\n', "\r\n").replace(' ', " \t ").into_bytes()
}

/// luce-obj's main.luc fixture: a grid whose faces follow each row of points,
/// with UVs and normals, so references cross chunk boundaries both ways.
pub fn chunked(n: i64) -> Vec<u8> {
    let mut lines = Vec::new();
    let nf = n as f64;
    for z in 0..n {
        for x in 0..n {
            let jitter = ((x * 7919 + z * 104729) % 2001 - 1000) as f64 * 1e-7;
            lines.push(format!(
                "v {} {} {}",
                x as f64 / nf - 0.5,
                jitter,
                z as f64 / nf - 0.5
            ));
            if (x + z) % 97 == 0 {
                lines.push(format!("vt {:.6} {:.25}", x as f64 / nf, z as f64 / nf));
            } else {
                lines.push(format!("vt {:.6} {:.6}", x as f64 / nf, z as f64 / nf));
            }
        }
        lines.push(if z % 40 != 0 {
            "vn 0 1 0".into()
        } else {
            "vn 0 1 0 # übergang".into()
        });
        if z > 0 {
            for x in 0..n - 1 {
                let a = (z - 1) * n + x + 1;
                let b = a + n;
                if x % 2 == 1 {
                    lines.push(format!(
                        "f {a}/{a}/{z} {b}/{b}/{} {}/{}/-1 {}/{}/{z}",
                        z + 1,
                        b + 1,
                        b + 1,
                        a + 1,
                        a + 1
                    ));
                } else {
                    lines.push(format!(
                        "f {}/{a} {}/{b} {}/{}",
                        a - z * n - n - 1,
                        b - z * n - n - 1,
                        b - z * n - n,
                        b + 1
                    ));
                }
            }
        }
    }
    (lines.join("\n") + "\n").into_bytes()
}

/// A 1000-corner concave star, as one face.
pub fn star() -> Vec<u8> {
    let mut text = String::new();
    let mut face = String::from("f");
    for at in 0..1000 {
        let radius = if at % 2 == 0 { 1.0 } else { 0.8 };
        let angle = 2.0 * std::f64::consts::PI * at as f64 / 1000.0;
        writeln!(
            text,
            "v {} {} 0",
            radius * angle.cos(),
            radius * angle.sin()
        )
        .unwrap();
        write!(face, " {}", at + 1).unwrap();
    }
    (text + &face + "\n").into_bytes()
}
