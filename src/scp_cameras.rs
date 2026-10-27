//! Spot camera sections (type 3) of the SCP blob, read as authored.
//!
//! A section holds `count` records of 48 bytes from its data offset, then `count * 2` 64-byte matrices; its size is
//! `data + count * 176`. The section's engine handler copies each record's two matrices into the object whose id the
//! record names, which is how it is known to be a placement of two frames. Record words (big-endian): `0` the
//! record's own id (the same hash in most records), `1`, `3`, `9`, `10` and `11` pool leftovers (not read), `2` the
//! hash of a spot name (`BigJumpOne`, `Chlg17_ObjSpot_Cam12_AnyTrick`: names of the database's spots), `4` the
//! parent id, `5` an f32 (53.638 in every record), `6` a flag (1 in every record), `7` and `8` the offsets, from the
//! section start, of the two matrices. That these are cameras is read from the spot names (`..._Cam12_...`) and the
//! frame pair; the engine function that uses them was not traced.
use crate::Affine;
use crate::scene::Unread;
use crate::scp_tables::{Section, finite, matrix, word};
use crate::{Result, element, invalid, offset};
use primase_foundations::bytes::View;

const RECORD: usize = 48;
const MATRIX: usize = 64;

#[derive(Clone, Debug, PartialEq)]
/// A spot camera record: two frames placed under a spot name.
pub struct SpotCamera {
    /// Offset in the SCP file of the section holding it ([`Section::offset`]).
    pub section: usize,
    /// Its 0-based index among the section's records.
    pub index: u32,
    /// Word 0: the record's own id, as stored.
    pub id: u32,
    /// Word 2: sdbm of a spot name as written.
    pub name: u32,
    /// Word 4: the parent id.
    pub parent: u32,
    /// Word 5, an f32 as stored (53.638 in every record seen).
    pub value: f32,
    /// Word 6, a flag word as stored (1 in every record seen).
    pub flag: u32,
    /// The first matrix (word 7): the frame the record places, which the second looks along. Unproven reading.
    pub camera: Affine,
    /// The second matrix (word 8): the frame of the point looked at. Unproven reading: its linear part is
    /// diag(-1, 1, 1) in 159 of 281 records.
    pub target: Affine,
}

/// Read every spot camera of one type 3 section; a section of the wrong size is reported unread and gives none.
pub fn decode(scp: &[u8], s: &Section, unread: &mut Vec<Unread>) -> Result<Vec<SpotCamera>> {
    let v = View::new(scp);
    let n = s.count as usize;
    if n.checked_mul(RECORD + 2 * MATRIX)
        .and_then(|b| b.checked_add(s.data))
        .is_none_or(|b| b != s.size)
    {
        unread.push(Unread {
            file: "scp",
            offset: s.offset,
            bytes: s.size,
            reason: "spot camera section whose size is not data offset + count * 176",
        });
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(n);
    let (low, high) = (element(s.data, n, RECORD, 0)?, s.size);
    let records = offset(s.offset, s.data)?;
    for index in 0..s.count {
        let at = element(records, index as usize, RECORD, 0)?;
        let record_word = |k: usize| word(v, element(at, k, 4, 0)?, "spot camera record");
        let mut frames = [Affine { cols: [[0.; 3]; 4] }; 2];
        for (frame, k) in frames.iter_mut().zip([7, 8]) {
            let rel = record_word(k)? as usize;
            if rel < low || !rel.is_multiple_of(4) || rel.checked_add(MATRIX).is_none_or(|e| e > high) {
                return Err(invalid(
                    element(at, k, 4, 0)?,
                    "scp: spot camera matrix outside its array",
                ));
            }
            *frame = matrix(v, offset(s.offset, rel)?)?;
        }
        let value = finite(
            v,
            offset(at, 20)?,
            "spot camera record",
            "scp: nonfinite spot camera value",
        )?;
        let [camera, target] = frames;
        out.push(SpotCamera {
            section: s.offset,
            index,
            id: record_word(0)?,
            name: record_word(2)?,
            parent: record_word(4)?,
            value,
            flag: record_word(6)?,
            camera,
            target,
        });
    }
    Ok(out)
}
