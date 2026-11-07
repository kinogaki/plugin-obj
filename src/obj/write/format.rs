//! Number text for the writer: the shortest decimal that reads back as the
//! same `f64` (via kinogaki-float-text), and plain integers.

/// Append the shortest round-trip spelling of `value`, dropping a trailing
/// `.0` (so `1.0` is written `1`).
#[inline]
pub fn float(out: &mut Vec<u8>, buffer: &mut kinogaki_float_text::FloatText, value: f64) {
    let text = buffer.format_bytes(value);
    let text = text.strip_suffix(b".0").unwrap_or(text);
    out.extend_from_slice(text);
}

/// Append `value` in decimal.
#[inline]
pub fn integer(out: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[at..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_are_short_and_exact() {
        let mut buffer = kinogaki_float_text::FloatText::new();
        for value in [
            0.0,
            -0.0,
            1.0,
            -2.5,
            0.1,
            0.30000000000000004,
            1e-7,
            1e22,
            1e300,
            -1e-300,
            5e-324,
            f64::MAX,
            f64::MIN_POSITIVE,
            123456789.0,
            1.0 / 3.0,
        ] {
            let mut out = Vec::new();
            float(&mut out, &mut buffer, value);
            let text = std::str::from_utf8(&out).unwrap();
            let back: f64 = text.parse().unwrap();
            assert_eq!(back.to_bits(), value.to_bits(), "{text}");
            assert!(!text.ends_with(".0"), "{text}");
        }
    }

    #[test]
    fn integers() {
        let mut out = Vec::new();
        for value in [0, 7, 10, 4294967295, u64::MAX] {
            out.clear();
            integer(&mut out, value);
            assert_eq!(out, value.to_string().as_bytes());
        }
    }
}
