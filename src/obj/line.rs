//! Tokens of one line. A line ends at a line feed; tokens are separated by
//! spaces, tabs and carriage returns; a token starting with `#` begins a
//! comment that runs to the end of the line.
use crate::obj::{
    MalformedKind,
    error::Fault,
    number::{parse_f64, token_end},
};

pub struct Line<'a> {
    pub bytes: &'a [u8],
    pub at: usize,
    pub end: usize,
}

#[inline(always)]
pub fn separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r')
}

impl<'a> Line<'a> {
    #[inline(always)]
    pub fn skip(&mut self) {
        while self.at < self.end && separator(self.bytes[self.at]) {
            self.at += 1;
        }
    }

    /// Whether another value follows (not the end of the line or a comment).
    #[inline(always)]
    pub fn more(&mut self) -> bool {
        self.skip();
        self.at < self.end && self.bytes[self.at] != b'#'
    }

    /// The next token, or `None` at the end of the line or a comment.
    #[inline(always)]
    pub fn token(&mut self) -> Option<&'a [u8]> {
        if !self.more() {
            return None;
        }
        let start = self.at;
        self.at = token_end(self.bytes, start, self.end);
        Some(&self.bytes[start..self.at])
    }

    /// The next number.
    #[inline(always)]
    pub fn number(&mut self) -> Result<f64, Fault> {
        if !self.more() {
            return Err(Fault::new(
                MalformedKind::Syntax,
                self.at,
                "expected a number",
            ));
        }
        let (value, stop) = parse_f64(self.bytes, self.at, self.end).ok_or_else(|| {
            Fault::new(
                MalformedKind::Number,
                self.at,
                "not a finite decimal number",
            )
        })?;
        self.at = stop;
        Ok(value)
    }

    /// The next number when one follows, else `default`.
    #[inline(always)]
    pub fn optional(&mut self, default: f64) -> Result<f64, Fault> {
        if self.more() {
            self.number()
        } else {
            Ok(default)
        }
    }

    /// Fails when anything but a comment follows.
    #[inline(always)]
    pub fn finish(&mut self, record: &'static str) -> Result<(), Fault> {
        if self.more() {
            return Err(Fault::new(MalformedKind::Syntax, self.at, record));
        }
        Ok(())
    }

    /// The rest of the line as text: up to a comment token, trimmed. Names
    /// keep inner spaces; invalid UTF-8 becomes U+FFFD.
    pub fn rest(&mut self) -> String {
        self.skip();
        let start = self.at;
        let mut stop = start;
        let mut at = start;
        while at < self.end {
            let byte = self.bytes[at];
            if byte == b'#' && (at == start || separator(self.bytes[at - 1])) {
                break;
            }
            at += 1;
            if !separator(byte) {
                stop = at;
            }
        }
        self.at = self.end;
        // A stray carriage return inside a name reads as a space, so every
        // name stays one line when written again.
        String::from_utf8_lossy(&self.bytes[start..stop]).replace('\r', " ")
    }
}
