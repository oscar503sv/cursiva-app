//! Small decoding helpers.

use crate::reader::ReadError;

/// Requires at least `n` parameter bytes.
pub(crate) fn need(p: &[u8], n: usize, what: &'static str) -> Result<(), ReadError> {
    if p.len() < n {
        return Err(ReadError::Decode {
            what,
            reason: format!("{} bytes, need {n}", p.len()),
        });
    }
    Ok(())
}

/// Records a non-fatal error as a note; propagates fatal ones.
pub(crate) fn soft<T>(
    r: Result<T, ReadError>,
    notes: &mut Vec<String>,
) -> Result<Option<T>, ReadError> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_fatal() => Err(e),
        Err(e) => {
            notes.push(e.to_string());
            Ok(None)
        }
    }
}

/// Hex of `p[from..]` with trailing zero bytes removed (long-report padding).
pub(crate) fn tail_hex(p: &[u8], from: usize) -> String {
    let tail = p.get(from..).unwrap_or(&[]);
    let end = tail.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    hidlab_hidpp::hex(&tail[..end])
}

/// Bits of `value` not covered by `known`, as `0x..`, or `None` if zero.
pub(crate) fn unknown_bits(value: u16, known: u16) -> Option<String> {
    let rest = value & !known;
    (rest != 0).then(|| format!("0x{rest:02x}"))
}

/// Names of the set bits of `value` found in `table`.
pub(crate) fn bit_names(value: u16, table: &[(u16, &'static str)]) -> Vec<&'static str> {
    table
        .iter()
        .filter(|(bit, _)| value & bit != 0)
        .map(|(_, n)| *n)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_hex_strips_padding() {
        assert_eq!(tail_hex(&[1, 2, 3, 0, 0], 1), "02 03");
        assert_eq!(tail_hex(&[1, 0, 0], 1), "");
        assert_eq!(tail_hex(&[1], 5), "");
    }

    #[test]
    fn bits() {
        assert_eq!(unknown_bits(0x0F, 0x0E), Some("0x01".into()));
        assert_eq!(unknown_bits(0x0E, 0x0E), None);
        assert_eq!(bit_names(0x05, &[(1, "a"), (2, "b"), (4, "c")]), ["a", "c"]);
    }
}
