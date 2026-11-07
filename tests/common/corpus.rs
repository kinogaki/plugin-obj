//! The named corpus the differential and round-trip tests run over: the
//! fixtures, the generated files, small valid cases and random text.

use crate::files::{chunked, fixture, large, mixed_crlf, star};
use crate::random::random_obj;

/// The small valid cases of luce-obj's reader_tests.luc.
pub const SMALL: &[&str] = &[
    "v 0 0 0 2\nv 2 0 0 2\nv 0 2 0 4\nf 1 2 3\n",
    "v 0 0 0\nv 1 0 0\nv 0 1 0\nf +1 +2 +3\n",
    "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvt 0.5 0.25\nf 1/1 2/1 3/1\nf 2 4 3\n",
    "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 1 1\nvn 0 0 1\nf 1 2 3\n",
    "v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\n\r\nf 1 2 3\r\n",
    "v 1 2 3\n",
    "v 0 0 0\nv 1 0 0\nv 2 0 0\nf 1 2 3\n",
    "  v\t0 0 0 # a point\n v 1 0 0\nv 0 1 0\nf 1 2 3 # the face\n#end",
    "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf -4/1 -3/2 -2/3 -1/4\n",
];

/// The named corpus for differential tests.
pub fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut corpus: Vec<(String, Vec<u8>)> = ["numbers", "polygons", "mixed", "accents"]
        .iter()
        .map(|name| (format!("{name}.obj"), fixture(&format!("{name}.obj"))))
        .collect();
    corpus.push(("mixed_crlf.obj".into(), mixed_crlf()));
    corpus.push(("large.obj".into(), large()));
    corpus.push(("chunked.obj".into(), chunked(160)));
    corpus.push(("star.obj".into(), star()));
    for (at, text) in SMALL.iter().enumerate() {
        corpus.push((format!("small{at}.obj"), text.as_bytes().to_vec()));
    }
    for seed in 1..=8 {
        corpus.push((
            format!("random{seed}.obj"),
            random_obj(seed, 2000 * seed as usize),
        ));
    }
    corpus
}
