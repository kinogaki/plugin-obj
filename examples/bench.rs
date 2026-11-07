//! Reading and writing benchmarks (docs/BENCHMARKS.md).
//!
//!   bench generate DIR [SIDE...]   quad grids SIDE x SIDE faces (default 1000
//!                                  and 3163: 1M and 10M faces), positions
//!                                  only and with vt/vn, %.6f numbers
//!   bench read [--runs N] FILE...  read_file: min and median wall time; also
//!                                  parse of text in memory on all threads and on one
//!
//! It runs as a host does: workers installed, one thread a processor (the
//! SDK testing support's, as Studio installs its own).
//!   bench write [--runs N] FILE... write of the file's model into memory and to a file
use kinogaki_studio_api_testing::workers;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut runs = 5;
    if let Some(at) = arguments.iter().position(|a| a == "--runs") {
        runs = arguments[at + 1].parse()?;
        arguments.drain(at..at + 2);
    }
    let Some(command) = arguments.first().cloned() else {
        return Err("usage: bench generate DIR [SIDE...] | read FILE... | write FILE...".into());
    };
    let rest = &arguments[1..];
    match command.as_str() {
        "generate" => {
            let directory = Path::new(&rest[0]);
            let sides: Vec<usize> = if rest.len() > 1 {
                rest[1..]
                    .iter()
                    .map(|s| s.parse())
                    .collect::<Result<_, _>>()?
            } else {
                vec![1000, 3163]
            };
            for side in sides {
                for attributes in [false, true] {
                    let name = format!("quad{side}{}.obj", if attributes { "_uvn" } else { "" });
                    grid(&directory.join(&name), side, attributes)?;
                    println!("wrote {name}");
                }
            }
        }
        "read" => {
            for path in rest {
                let text = std::fs::read(path)?;
                let obj = kinogaki_obj::obj::parse(&text)?;
                workers::install(0);
                let file = time(runs, || drop(kinogaki_obj::obj::read_file(path).unwrap()));
                let memory = time(runs, || drop(kinogaki_obj::obj::parse(&text).unwrap()));
                workers::install(1);
                let single = time(runs.min(3), || {
                    drop(kinogaki_obj::obj::parse(&text).unwrap())
                });
                println!(
                    "{path}: {} faces, {} MiB; read_file {file}; parse {memory}; parse 1 thread {single}",
                    obj.faces.len(),
                    text.len() >> 20
                );
            }
        }
        "write" => {
            workers::install(0);
            for path in rest {
                let obj = kinogaki_obj::obj::read_file(path)?;
                let mut size = 0;
                let memory = time(runs, || {
                    size = kinogaki_obj::obj::to_bytes(&obj).unwrap().len()
                });
                let target =
                    std::env::temp_dir().join(format!("obj-bench-{}.obj", std::process::id()));
                let file = time(runs, || {
                    let options = kinogaki_obj::obj::WriteOptions::default();
                    kinogaki_obj::obj::write_file(&target, &obj, &options).unwrap()
                });
                std::fs::remove_file(&target)?;
                println!(
                    "{path}: {} faces, {} MiB written; to_bytes {memory}; write_file {file}",
                    obj.faces.len(),
                    size >> 20
                );
            }
        }
        _ => return Err(format!("unknown command {command}").into()),
    }
    Ok(())
}

/// "min X ms, median Y ms (N runs)".
fn time(runs: usize, mut work: impl FnMut()) -> String {
    let mut samples: Vec<f64> = (0..runs.max(1))
        .map(|_| {
            let start = Instant::now();
            work();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    format!(
        "min {:.1} ms, median {:.1} ms ({} runs)",
        samples[0],
        samples[samples.len() / 2],
        samples.len()
    )
}

/// A side x side quad grid on (side + 1)^2 points with a small height wave;
/// with `attributes`, one vt per point and a shared vn, `p/t/n` corners.
fn grid(path: &Path, side: usize, attributes: bool) -> std::io::Result<()> {
    let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
    let n = side + 1;
    for z in 0..n {
        for x in 0..n {
            let y = 0.02 * ((x + z) as f64 * 0.1).sin();
            writeln!(
                out,
                "v {:.6} {:.6} {:.6}",
                x as f64 / side as f64 - 0.5,
                y,
                z as f64 / side as f64 - 0.5
            )?;
        }
    }
    if attributes {
        for z in 0..n {
            for x in 0..n {
                writeln!(
                    out,
                    "vt {:.6} {:.6}",
                    x as f64 / side as f64,
                    z as f64 / side as f64
                )?;
            }
        }
        writeln!(out, "vn 0 1 0")?;
    }
    for z in 0..side {
        for x in 0..side {
            let a = z * n + x + 1;
            let ring = [a, a + n, a + n + 1, a + 1];
            if attributes {
                writeln!(
                    out,
                    "f {0}/{0}/1 {1}/{1}/1 {2}/{2}/1 {3}/{3}/1",
                    ring[0], ring[1], ring[2], ring[3]
                )?;
            } else {
                writeln!(out, "f {} {} {} {}", ring[0], ring[1], ring[2], ring[3])?;
            }
        }
    }
    out.flush()
}
