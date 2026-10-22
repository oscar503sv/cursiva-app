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
