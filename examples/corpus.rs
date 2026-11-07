//! The differential-test corpus, for tools/luce-oracle/refresh.sh:
//!
//!   corpus write DIR          write every corpus file into DIR
//!   corpus fingerprints DIR   print `name<TAB>fingerprint` for each file from
//!                             luce-obj's output DIR/NAME.canon, or
//!                             `name<TAB>ERROR message` from DIR/NAME.error
#[path = "../tests/common/corpus.rs"]
mod corpus;
#[path = "../tests/common/files.rs"]
mod files;
#[path = "../tests/common/fingerprint.rs"]
mod fingerprint;
#[path = "../tests/common/random.rs"]
mod random;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    let (Some(command), Some(directory)) = (arguments.get(1), arguments.get(2)) else {
        return Err("usage: corpus write|fingerprints DIR".into());
    };
    let directory = Path::new(directory);
    for (name, text) in corpus::corpus() {
        match command.as_str() {
            "write" => std::fs::write(directory.join(&name), text)?,
            "fingerprints" => {
                let error = directory.join(format!("{name}.error"));
                if error.exists() {
                    println!("{name}\tERROR {}", std::fs::read_to_string(error)?.trim());
                } else {
                    let canonical = std::fs::read(directory.join(format!("{name}.canon")))?;
                    println!("{name}\t{}", oracle_fingerprint(&canonical));
                }
            }
            _ => return Err(format!("unknown command {command}").into()),
        }
    }
    Ok(())
}

/// The fingerprint of luce-obj's own output for a file: its `Obj.write`
/// text, read back, where every corner has its own vt/vn record.
fn oracle_fingerprint(canonical: &[u8]) -> String {
    let obj = kinogaki_obj::obj::parse(canonical).expect("luce-obj output parses");
    fingerprint::hash(&obj, &obj.positions)
}
