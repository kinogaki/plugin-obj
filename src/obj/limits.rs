//! Budgets for untrusted input. Every table is sized from counted records
//! before it is filled, so these bound the reader's allocations.

/// Size limits checked while reading. The defaults admit files of several
/// gigabytes; lower them for untrusted sources on small devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Total input bytes.
    pub input_bytes: u64,
    /// `v`, `vt` and `vn` records, each.
    pub vertices: usize,
    /// `f`, `l` and `p` records together.
    pub elements: usize,
    /// Corners over all elements (at most `u32::MAX`, the offset width).
    pub corners: usize,
    /// Bytes of one line (names and the longest n-gon included).
    pub line_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 16 << 30,
            vertices: 1 << 30,
            elements: 1 << 30,
            corners: (u32::MAX - 1) as usize,
            line_bytes: 64 << 20,
        }
    }
}

/// How to read: limits and parallelism.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOptions {
    pub limits: Limits,
    /// Worker threads; 0 uses the available parallelism.
    pub threads: usize,
    /// Bytes read and parsed at a time when streaming from a file or reader.
    pub block_bytes: usize,
    /// The smallest chunk handed to one thread; smaller inputs parse on one.
    pub chunk_bytes: usize,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            threads: 0,
            block_bytes: 32 << 20,
            chunk_bytes: 256 << 10,
        }
    }
}

/// The worker count for `requested` (0 = all available).
pub(crate) fn thread_count(requested: usize) -> usize {
    if requested > 0 {
        requested
    } else {
        std::thread::available_parallelism().map_or(1, usize::from)
    }
}
