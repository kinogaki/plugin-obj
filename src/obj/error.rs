//! Errors with the line and column where reading stopped.
use std::{borrow::Cow, fmt, io};

/// Why reading or writing failed.
#[derive(Debug)]
pub enum ObjError {
    /// The underlying reader or writer failed.
    Io(io::Error),
    /// The text is not valid OBJ/MTL, or exceeds a [`crate::obj::Limits`] budget.
    Malformed(Malformed),
}

/// What kind of problem a [`Malformed`] error reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MalformedKind {
    /// A record has the wrong shape: missing or extra values, bad tokens.
    Syntax,
    /// A number is not a decimal, or not finite.
    Number,
    /// A reference is zero or names an element not declared before it.
    Index,
    /// The input exceeds a configured budget.
    Limit,
    /// A recognised construct this library does not read (line continuations).
    Unsupported,
}

/// A malformed input, with its 1-based line and byte column (0 when the
/// problem is not tied to a place, such as a total budget).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed {
    pub kind: MalformedKind,
    pub line: u64,
    pub column: u64,
    pub message: Cow<'static, str>,
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Malformed(error) => error.fmt(f),
        }
    }
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(
                f,
                "line {}, column {}: {}",
                self.line, self.column, self.message
            )
        }
    }
}

impl std::error::Error for ObjError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Malformed(_) => None,
        }
    }
}

impl From<io::Error> for ObjError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<Malformed> for ObjError {
    fn from(error: Malformed) -> Self {
        Self::Malformed(error)
    }
}

impl ObjError {
    /// The malformed-input details, when that is what failed.
    pub fn malformed(&self) -> Option<&Malformed> {
        match self {
            Self::Malformed(error) => Some(error),
            Self::Io(_) => None,
        }
    }
}

/// A problem found inside a chunk, at a byte offset into the text being parsed.
/// The reader turns it into a [`Malformed`] with a line and column.
#[derive(Clone, Debug)]
pub(crate) struct Fault {
    pub kind: MalformedKind,
    pub at: usize,
    pub message: Cow<'static, str>,
}

impl Fault {
    pub fn new(kind: MalformedKind, at: usize, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind,
            at,
            message: message.into(),
        }
    }

    /// The fault as a [`Malformed`] error: `text` is the block it was found
    /// in, whose first byte is on line `first_line`.
    pub fn locate(self, text: &[u8], first_line: u64) -> Malformed {
        let before = &text[..self.at.min(text.len())];
        let line_start = kinogaki_bytes::rfind(b'\n', before).map_or(0, |at| at + 1);
        Malformed {
            kind: self.kind,
            line: first_line + kinogaki_bytes::count(b'\n', before) as u64,
            column: (self.at - line_start) as u64 + 1,
            message: self.message,
        }
    }
}

/// A budget error not tied to a place in the text.
pub(crate) fn limit(message: impl Into<Cow<'static, str>>) -> Malformed {
    Malformed {
        kind: MalformedKind::Limit,
        line: 0,
        column: 0,
        message: message.into(),
    }
}
