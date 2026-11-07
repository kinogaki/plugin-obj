//! Malformed input never panics, and the outcome does not depend on how the
//! text is split: random and mutated inputs are read on one thread, in many
//! small chunks and streamed in small blocks, and must give the same model or
//! the same error; models that read must validate and round-trip.
#[path = "common/corpus.rs"]
mod corpus;
#[path = "common/files.rs"]
mod files;
#[path = "common/random.rs"]
mod random;
use kinogaki_obj::obj::{Obj, ObjError, ReadOptions, mtl, parse, parse_with, read, to_bytes};
use kinogaki_studio_api::format::{ReadBudget, ReadLimits};
use random::Random;

fn outcome(result: Result<Obj, ObjError>) -> Result<Obj, String> {
    result.map_err(|error| match error {
        ObjError::Malformed(error) => format!("{error:?}"),
        ObjError::Io(error) => panic!("unexpected I/O error {error}"),
        ObjError::Cancelled => panic!("never cancelled"),
    })
}

fn check(text: &[u8]) {
    // Each read its own budget (clones share what was spent), lines up to
    // 64 KiB.
    let budget = || {
        ReadBudget::new(ReadLimits {
            allocation: 1 << 16,
            ..ReadLimits::default()
        })
    };
    let one = outcome(parse_with(
        text,
        ReadOptions {
            threads: 1,
            budget: budget(),
            ..ReadOptions::default()
        },
    ));
    let many = outcome(parse_with(
        text,
        ReadOptions {
            threads: 6,
            chunk_bytes: 64,
            budget: budget(),
            ..ReadOptions::default()
        },
    ));
    assert_eq!(one, many, "{}", String::from_utf8_lossy(text));
    let streamed = outcome(read(
        text,
        ReadOptions {
            threads: 3,
            chunk_bytes: 32,
            block_bytes: 300,
            budget: budget(),
        },
    ));
    // A streamed line-length limit is reported without a place; otherwise alike.
    if !matches!(&one, Err(message) if message.contains("Limit")) {
        assert_eq!(one, streamed, "{}", String::from_utf8_lossy(text));
    }
    if let Ok(mut obj) = one {
        obj.validate().unwrap();
        // Skipped statements are reported, not written.
        obj.skipped.clear();
        assert_eq!(parse(&to_bytes(&obj).unwrap()).unwrap(), obj);
    }
    // MTL parsing is total too.
    let _ = mtl::parse(text);
}

/// Rounds scale with OBJ_FUZZ (default 1) for longer local runs.
fn rounds(base: usize) -> usize {
    base * std::env::var("OBJ_FUZZ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

const ALPHABET: &[u8] = b"vtnfplogsu/-+.e# \t\r\n0123456789\\x";

fn mutate(random: &mut Random, text: &mut Vec<u8>) {
    for _ in 0..1 + random.below(8) {
        let at = if text.is_empty() {
            0
        } else {
            random.below(text.len() as u64) as usize
        };
        match random.below(6) {
            0 if !text.is_empty() => {
                text[at] = ALPHABET[random.below(ALPHABET.len() as u64) as usize]
            }
            1 => text.insert(at, ALPHABET[random.below(ALPHABET.len() as u64) as usize]),
            2 if !text.is_empty() => {
                text.remove(at);
            }
            3 => text.truncate(at),
            4 if !text.is_empty() => text[at] = random.next() as u8,
            _ => {
                let end = (at + random.below(40) as usize).min(text.len());
                let piece = text[at..end].to_vec();
                let to = random.below(text.len() as u64 + 1) as usize;
                text.splice(to..to, piece);
            }
        }
    }
}

#[test]
fn mutated_corpus_files() {
    let mut random = Random::new(99 + rounds(1) as u64);
    let corpus = corpus::corpus();
    for round in 0..rounds(600) {
        let (_, text) = &corpus[round % corpus.len()];
        // Mutate a window so large files stay quick.
        let start = random.below(text.len().max(1) as u64) as usize;
        let mut sample = text[start.min(text.len())..(start + 3000).min(text.len())].to_vec();
        mutate(&mut random, &mut sample);
        check(&sample);
    }
}

#[test]
fn random_token_soup() {
    let mut random = Random::new(7);
    let words: &[&[u8]] = &[
        b"v", b"vt", b"vn", b"f", b"l", b"p", b"o", b"g", b"s", b"usemtl", b"mtllib", b"vp", b"#",
        b"1", b"-1", b"2/1", b"1//1", b"3/2/1", b"0", b"-0.5e3", b"1e400", b"nan", b"off", b"\\",
        b"/", b"//", b"name",
    ];
    for _ in 0..rounds(2000) {
        let mut text = Vec::new();
        for _ in 0..random.below(60) {
            text.extend_from_slice(words[random.below(words.len() as u64) as usize]);
            text.push(b"  \t\n\n\r"[random.below(6) as usize]);
        }
        check(&text);
    }
}

#[test]
fn random_bytes() {
    let mut random = Random::new(3);
    for _ in 0..rounds(500) {
        let text: Vec<u8> = (0..random.below(400))
            .map(|_| random.next() as u8)
            .collect();
        check(&text);
    }
}
