//! Occluder sections (type b) of the SCP blob, read as authored.
//!
//! A section holds `count` records of 16 bytes from its data offset, then for each record in order a 64-byte matrix
//! and an outline of line segments (each end a vec4 of f32 x, y, z and w = 1). Record words (big-endian): `0` the
//! name hash (sdbm of `RMOccluder`, `RMOccluder<N>` or, for 65 of the 341 on the disc, a name not known), `1` the offset of the record's matrix from the section start, `2` the offset
//! of its outline (the matrix's plus 64), `3` flags. The low half of the flags says the outline's shape: `0x0800` is
//! four segments (a rectangle's outline, 128 bytes) and `0x1801` twelve (a box's edges, 384 bytes); the high half
//! varies from record to record and is not understood, so it is kept. The matrices and outlines tile the section
//! exactly. The only consumer found is the Lua binding `set_occluder_enabled`; the engine has no section handler
//! for the type, so what the outline is measured in (the matrix's local space, by the stored values) is read from
//! the data, not from the engine.
use crate::Affine;
use crate::scene::Unread;
use crate::scp_tables::{Section, finite, float, matrix, word};
use crate::{Result, element, invalid, offset};
use primase_foundations::bytes::View;

const RECORD: usize = 16;
const MATRIX: usize = 64;
const SEGMENT: usize = 32;

/// The low flag half of an outline of four segments.
pub const SHAPE_RECTANGLE: u32 = 0x0800;
/// The low flag half of an outline of twelve segments.
pub const SHAPE_BOX: u32 = 0x1801;

fn segments(flags: u32) -> Option<usize> {
    match flags & 0xffff {
        SHAPE_RECTANGLE => Some(4),
        SHAPE_BOX => Some(12),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
/// An occluder: a world matrix and the line segments of its outline.
pub struct Occluder {
    /// Offset in the SCP file of the section holding it ([`Section::offset`]).
    pub section: usize,
    /// Its 0-based index among the section's records.
    pub index: u32,
    /// Word 0: the occluder's name hash, as stored.
    pub name: u32,
    /// Word 3 as stored: the low half is the shape ([`SHAPE_RECTANGLE`], [`SHAPE_BOX`]), the high half unknown.
    pub flags: u32,
    /// The world matrix the record points at.
    pub world: Affine,
    /// The outline's segments, each as its two end points (x, y, z), as stored.
    pub outline: Vec<[[f32; 3]; 2]>,
}

/// Read every occluder of one type 0xb section; a section that does not tile is reported unread and gives none.
pub fn decode(scp: &[u8], s: &Section, unread: &mut Vec<Unread>) -> Result<Vec<Occluder>> {
    let v = View::new(scp);
    let n = s.count as usize;
    let mut refuse = |reason| {
        unread.push(Unread {
            file: "scp",
            offset: s.offset,
            bytes: s.size,
            reason,
        });
        Ok(Vec::new())
    };
    let Some(records) = n
        .checked_mul(RECORD)
        .and_then(|b| b.checked_add(s.data))
        .filter(|&b| b <= s.size)
    else {
        return refuse("occluder section whose records do not fit");
    };
    let mut next = records;
    let mut out = Vec::with_capacity(n);
    let first = offset(s.offset, s.data)?;
    for index in 0..s.count {
        let at = element(first, index as usize, RECORD, 0)?;
        let record_word = |k: usize| word(v, element(at, k, 4, 0)?, "occluder record");
        let Some(edges) = segments(record_word(3)?) else {
            return refuse("occluder section with an outline shape not decoded");
        };
        let len = element(MATRIX, edges, SEGMENT, 0)?;
        let outline_at = offset(next, MATRIX)?;
        if record_word(1)? as usize != next
            || record_word(2)? as usize != outline_at
            || offset(next, len)? > s.size
        {
            return Err(invalid(
                offset(at, 4)?,
                "scp: occluder matrix and outline off their place",
            ));
        }
        let (m, o) = (offset(s.offset, next)?, offset(s.offset, outline_at)?);
        let mut outline = Vec::with_capacity(edges);
        for e in 0..edges {
            let mut seg = [[0.; 3]; 2];
            for (end, p) in seg.iter_mut().enumerate() {
                let q = offset(element(o, e, SEGMENT, 0)?, element(0, end, 16, 0)?)?;
                for (k, x) in p.iter_mut().enumerate() {
                    *x = finite(
                        v,
                        element(q, k, 4, 0)?,
                        "occluder outline",
                        "scp: nonfinite occluder outline",
                    )?;
                }
                if float(v, offset(q, 12)?, "occluder outline")? != 1. {
                    return Err(invalid(offset(q, 12)?, "scp: occluder outline w is not 1"));
                }
            }
            outline.push(seg);
        }
        out.push(Occluder {
            section: s.offset,
            index,
            name: record_word(0)?,
            flags: record_word(3)?,
            world: matrix(v, m)?,
            outline,
        });
        next = offset(next, len)?;
    }
    if next != s.size {
        return refuse("occluder section whose matrices and outlines do not tile it");
    }
    Ok(out)
}
