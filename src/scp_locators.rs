//! Locator sections (type 5) of the SCP blob, read as authored.
//!
//! A section holds `count` records of 16 bytes from its data offset, then `count` 64-byte matrices; its size is
//! `data + count * 80`. Record words (big-endian): `0` a class hash (sdbm of `RMLocator` in the one record there is),
//! `1` the name hash, `2` a kind hash (sdbm of `Waypoint`), `3` the matrix's offset from the section start. The
//! files hold one such record (the `echo` level's); the class and kind are named by their hashes.
use crate::Affine;
use crate::scene::Unread;
use crate::scp_tables::{Section, matrix, word};
use crate::{Result, element, invalid, offset};
use primase_foundations::bytes::View;

const RECORD: usize = 16;
const MATRIX: usize = 64;

#[derive(Clone, Debug, PartialEq)]
/// A locator record: a class, a name and a kind under one world matrix.
pub struct Locator {
    /// Offset of the section holding it.
    pub section: usize,
    /// Its index in the section.
    pub index: u32,
    /// Word 0: the class hash.
    pub class: u32,
    /// Word 1: the name hash.
    pub name: u32,
    /// Word 2: the kind hash.
    pub kind: u32,
    /// The matrix word 3 points at.
    pub world: Affine,
}

/// The class and kind names whose hashes the locator record carries (sdbm of the name).
pub const CLASS_LOCATOR: &str = "RMLocator";
/// The kind name whose sdbm the locator record carries.
pub const KIND_WAYPOINT: &str = "Waypoint";

/// Read every locator of one type 5 section; a section of the wrong size is reported unread and gives none.
pub fn decode(scp: &[u8], s: &Section, unread: &mut Vec<Unread>) -> Result<Vec<Locator>> {
    let v = View::new(scp);
    let n = s.count as usize;
    if n.checked_mul(RECORD + MATRIX)
        .and_then(|b| b.checked_add(s.data))
        .is_none_or(|b| b != s.size)
    {
        unread.push(Unread {
            file: "scp",
            offset: s.offset,
            bytes: s.size,
            reason: "locator section whose size is not data offset + count * 80",
        });
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(n);
    let records = offset(s.offset, s.data)?;
    let matrices = element(s.data, n, RECORD, 0)?;
    for index in 0..s.count {
        let at = element(records, index as usize, RECORD, 0)?;
        let record_word = |k: usize| word(v, element(at, k, 4, 0)?, "locator record");
        let rel = record_word(3)? as usize;
        if rel < matrices || !rel.is_multiple_of(4) || rel.checked_add(MATRIX).is_none_or(|e| e > s.size) {
            return Err(invalid(offset(at, 12)?, "scp: locator matrix outside its array"));
        }
        out.push(Locator {
            section: s.offset,
            index,
            class: record_word(0)?,
            name: record_word(1)?,
            kind: record_word(2)?,
            world: matrix(v, offset(s.offset, rel)?)?,
        });
    }
    Ok(out)
}
