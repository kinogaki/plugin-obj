//! Exact decimal parsing straight from the file bytes.
//!
//! Most OBJ numbers have at most 15 significant digits and a small decimal
//! exponent; for those one correctly rounded multiply or divide of two exactly
//! representable values gives the correctly rounded result (Clinger's fast
//! path). Everything else (long mantissas, far exponents, `inf`/`nan`
//! spellings) goes to the standard library's correctly rounded parser with the
//! whole token, so every value is bit-identical to `str::parse::<f64>`.

/// Powers of ten that `f64` holds exactly.
const POWERS: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];
/// Integers up to 2^53 are exact in `f64`.
const EXACT_MANTISSA: u64 = 1 << 53;

/// Whether `byte` ends a token.
#[inline(always)]
pub fn delimiter(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// The end of the token starting at `at` (`end` bounds the line).
#[inline(always)]
pub fn token_end(bytes: &[u8], mut at: usize, end: usize) -> usize {
    while at < end && !delimiter(bytes[at]) {
        at += 1;
    }
    at
}

/// The number spelled by the token starting at `start`, and the token's end.
/// `None` when the token is not a number or its value is not finite.
#[inline]
pub fn parse_f64(bytes: &[u8], start: usize, end: usize) -> Option<(f64, usize)> {
    if let Some((value, stop)) = fast(bytes, start, end)
        && (stop == end || delimiter(bytes[stop]))
    {
        return Some((value, stop));
    }
    slow(bytes, start, end)
}

/// Clinger's fast path for `[+-]digits[.digits][(e|E)[+-]digits]`.
#[inline(always)]
fn fast(bytes: &[u8], start: usize, end: usize) -> Option<(f64, usize)> {
    let mut at = start;
    let negative = at < end && bytes[at] == b'-';
    if at < end && (bytes[at] == b'-' || bytes[at] == b'+') {
        at += 1;
    }
    let mut mantissa: u64 = 0;
    let mut digits = 0u32;
    let mut scale: i64 = 0;
    let mut seen = false;
    while at < end && bytes[at].is_ascii_digit() {
        let digit = u64::from(bytes[at] - b'0');
        seen = true;
        if mantissa != 0 || digit != 0 {
            if digits == 19 {
                return None;
            }
            mantissa = mantissa * 10 + digit;
            digits += 1;
        }
        at += 1;
    }
    if at < end && bytes[at] == b'.' {
        at += 1;
        while at < end && bytes[at].is_ascii_digit() {
            let digit = u64::from(bytes[at] - b'0');
            seen = true;
            scale -= 1;
            if mantissa != 0 || digit != 0 {
                if digits == 19 {
                    return None;
                }
                mantissa = mantissa * 10 + digit;
                digits += 1;
            }
            at += 1;
        }
    }
    if !seen {
        return None;
    }
    if at < end && (bytes[at] | 0x20) == b'e' {
        at += 1;
        let down = at < end && bytes[at] == b'-';
        if at < end && (bytes[at] == b'-' || bytes[at] == b'+') {
            at += 1;
        }
        let first = at;
        let mut exponent: i64 = 0;
        while at < end && bytes[at].is_ascii_digit() {
            if at - first == 6 {
                return None;
            }
            exponent = exponent * 10 + i64::from(bytes[at] - b'0');
            at += 1;
        }
        if at == first {
            return None;
        }
        scale += if down { -exponent } else { exponent };
    }
    let value = if mantissa == 0 {
        0.0
    } else if mantissa <= EXACT_MANTISSA && (-22..=22).contains(&scale) {
        let m = mantissa as f64;
        if scale >= 0 {
            m * POWERS[scale as usize]
        } else {
            m / POWERS[(-scale) as usize]
        }
    } else {
        return None;
    };
    Some((if negative { -value } else { value }, at))
}

/// The general parser for what the fast path declines.
#[cold]
fn slow(bytes: &[u8], start: usize, end: usize) -> Option<(f64, usize)> {
    let stop = token_end(bytes, start, end);
    let text = std::str::from_utf8(&bytes[start..stop]).ok()?;
    // Rust accepts "inf"/"nan" spellings; OBJ values must be finite.
    let value: f64 = text.parse().ok()?;
    value.is_finite().then_some((value, stop))
}

/// A reference field: `[+-]digits` ending at a delimiter, or at `/` when
/// `slash`. Returns the signed value (`None` when malformed) and the field's end.
#[inline(always)]
pub fn parse_index(bytes: &[u8], start: usize, end: usize, slash: bool) -> (Option<i64>, usize) {
    let mut at = start;
    let negative = at < end && bytes[at] == b'-';
    if at < end && (bytes[at] == b'-' || bytes[at] == b'+') {
        at += 1;
    }
    let first = at;
    let mut value: i64 = 0;
    while at < end && bytes[at].is_ascii_digit() {
        if at - first == 18 {
            return (None, field_end(bytes, at, end, slash));
        }
        value = value * 10 + i64::from(bytes[at] - b'0');
        at += 1;
    }
    let stop = field_end(bytes, at, end, slash);
    if at == first || stop != at {
        return (None, stop);
    }
    (Some(if negative { -value } else { value }), stop)
}

#[inline(always)]
fn field_end(bytes: &[u8], mut at: usize, end: usize, slash: bool) -> usize {
    while at < end && !delimiter(bytes[at]) && !(slash && bytes[at] == b'/') {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<f64> {
        parse_f64(text.as_bytes(), 0, text.len()).map(|(value, _)| value)
    }

    #[test]
    fn matches_the_standard_parser() {
        for text in [
            "0",
            "-0",
            "-0.0",
            "1",
            "+1.5",
            ".5",
            "5.",
            "1e0",
            "007.25",
            "-.125e+3",
            "1E5",
            "0.1",
            "0.30000000000000004",
            "9007199254740993",
            "9007199254740995",
            "4.9406564584124654e-320",
            "2.2250738585072014e-308",
            "1.7976931348623157e308",
            "1e-400",
            "1.00000000000000011102230246251565404236316680908203125",
            "123456789012345678901234567890",
            "0.000000000000000000000000001",
            "1e22",
            "1e23",
            "-12345.678901e-5",
        ] {
            let expected: f64 = text.parse().unwrap();
            assert_eq!(
                parse(text).map(f64::to_bits),
                Some(expected.to_bits()),
                "{text}"
            );
        }
    }

    #[test]
    fn rejects_non_numbers_and_non_finite_values() {
        for text in [
            "", "-", "e5", "1e", "1x", "inf", "nan", "1e999", "--1", "1.2.3", "0x10",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    #[test]
    fn stops_at_delimiters() {
        let text = b"1.5 2";
        assert_eq!(parse_f64(text, 0, text.len()), Some((1.5, 3)));
        assert_eq!(parse_index(b"12/3", 0, 4, true), (Some(12), 2));
        assert_eq!(parse_index(b"-4 ", 0, 3, false), (Some(-4), 2));
        assert_eq!(parse_index(b"+0", 0, 2, false), (Some(0), 2));
        assert_eq!(parse_index(b"4x", 0, 2, false), (None, 2));
        assert_eq!(parse_index(b"", 0, 0, true), (None, 0));
    }
}
