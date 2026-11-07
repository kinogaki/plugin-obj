//! The counting pass: how many `v`, `vt`, `vn` and `f` records (and face
//! corners) a chunk holds, so every table is allocated once at its final size
//! and each chunk knows where its records go. It recognises keywords exactly
//! as the parsing pass does; anything it miscounts the parsing pass rejects.
use crate::obj::number::delimiter;

#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    pub newlines: u64,
    pub positions: usize,
    pub texcoords: usize,
    pub normals: usize,
    pub faces: usize,
    pub corners: usize,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.newlines += other.newlines;
        self.positions += other.positions;
        self.texcoords += other.texcoords;
        self.normals += other.normals;
        self.faces += other.faces;
        self.corners += other.corners;
    }
}

/// The records in `bytes`, a run of whole lines.
pub fn count(bytes: &[u8]) -> Counts {
    let mut counts = Counts::default();
    let end = bytes.len();
    let mut at = 0;
    while at < end {
        let stop = kinogaki_bytes::find(b'\n', &bytes[at..]).map_or(end, |offset| at + offset);
        let mut p = at;
        while p < stop && matches!(bytes[p], b' ' | b'\t' | b'\r') {
            p += 1;
        }
        // The keyword is bytes[p..k]; only one- and two-letter ones are counted.
        let ends_at = |k: usize| k == stop || delimiter(bytes[k]);
        if p < stop {
            match bytes[p] {
                b'v' if ends_at(p + 1) => counts.positions += 1,
                b'v' if p + 2 <= stop && bytes[p + 1] == b't' && ends_at(p + 2) => {
                    counts.texcoords += 1
                }
                b'v' if p + 2 <= stop && bytes[p + 1] == b'n' && ends_at(p + 2) => {
                    counts.normals += 1
                }
                b'f' if ends_at(p + 1) => {
                    counts.faces += 1;
                    counts.corners += tokens(bytes, p + 1, stop);
                }
                _ => {}
            }
        }
        if stop < end {
            counts.newlines += 1;
        }
        at = stop + 1;
    }
    counts
}

/// Tokens in bytes[at..stop] before any token that starts a comment.
fn tokens(bytes: &[u8], mut at: usize, stop: usize) -> usize {
    let mut count = 0;
    loop {
        while at < stop && matches!(bytes[at], b' ' | b'\t' | b'\r') {
            at += 1;
        }
        if at == stop || bytes[at] == b'#' {
            return count;
        }
        count += 1;
        while at < stop && !delimiter(bytes[at]) {
            at += 1;
        }
    }
}
