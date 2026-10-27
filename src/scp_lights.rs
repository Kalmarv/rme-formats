//! Light sections (type 4) of the SCP blob, read as authored.
//!
//! A section holds `count` records of 80 bytes from its data offset, then `count` 64-byte matrices; its size is
//! `data + count * 144`. Record words (big-endian): `0..3` colour as f32 r, g, b, a (a is 1 in every record), `4` the
//! name hash, `5..7` three f32 parameters, `8` a kind code, `9..12` four words (-1, 0, 0, -1 or -2 in the lights seen),
//! `13` the matrix's offset from the section start, `14` and `15` two words (0 except for one kind), `16..19` a
//! 16-byte buffer holding leftover text and numbers. The records say what a light is made of; what they mean
//! as a renderer's light (units, range, cone, which axis shines) is not established, so nothing here claims to be one.
use crate::Affine;
use crate::scene::Unread;
use crate::scp_tables::{Section, finite, matrix, word};
use crate::{Result, element, invalid, offset};
use primase_foundations::bytes::{Field, View};

const RECORD: usize = 80;
const MATRIX: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLight {
    pub section: usize,
    pub index: u32,
    /// Word 8, by count in the level SCPs: 0 (5,251), 7 (4,079), 8 (398), 1 (23, named `Sun` where the name
    /// resolves), 4 (23), 3 (4, with a texture name in the buffer: `gobo_star`, `gobo_circle`). Not a proven vocabulary.
    pub kind: u32,
    /// Red, green, blue, alpha as stored.
    pub rgba: [f32; 4],
    /// Word 4.
    pub name: u32,
    /// Words 5 to 7 as stored (kind 0: 0.2 to 11, 1 to 15 and 0.2 to 2.2; kinds 1 and 4: 1, 100 and 0.4 to 1).
    pub params: [f32; 3],
    /// Words 9 to 12 and 14, 15.
    pub links: [u32; 4],
    pub tail_words: [u32; 2],
    /// Words 16 to 19: leftover bytes of the editor's name buffer, kept raw.
    pub buffer: [u8; 16],
    /// The matrix at word 13. For kinds 7 and 8 it carries the volume's scale; kind 1's translation is not the sun's.
    pub world: Affine,
}

pub fn decode(scp: &[u8], s: &Section, unread: &mut Vec<Unread>) -> Result<Vec<SceneLight>> {
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
            reason: "light section whose size is not data offset + count * 144",
        });
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(n);
    let records = offset(s.offset, s.data)?;
    let matrices = element(s.data, n, RECORD, 0)?;
    for index in 0..s.count {
        let at = element(records, index as usize, RECORD, 0)?;
        let record_word = |k: usize| word(v, element(at, k, 4, 0)?, "light record");
        let float = |k: usize| {
            finite(
                v,
                element(at, k, 4, 0)?,
                "light record",
                "scp: nonfinite light value",
            )
        };
        let rel = record_word(13)? as usize;
        if rel < matrices || rel.checked_add(MATRIX).is_none_or(|e| e > s.size) {
            return Err(invalid(offset(at, 52)?, "scp: light matrix outside its array"));
        }
        let world = matrix(v, offset(s.offset, rel)?)?;
        let buffer_at = offset(at, 64)?;
        let buffer = <[u8; 16]>::try_from(v.slice(buffer_at, 16).field("light buffer")?)
            .map_err(|_| invalid(buffer_at, "scp: light buffer is not 16 bytes"))?;
        out.push(SceneLight {
            section: s.offset,
            index,
            kind: record_word(8)?,
            rgba: [float(0)?, float(1)?, float(2)?, float(3)?],
            name: record_word(4)?,
            params: [float(5)?, float(6)?, float(7)?],
            links: [
                record_word(9)?,
                record_word(10)?,
                record_word(11)?,
                record_word(12)?,
            ],
            tail_words: [record_word(14)?, record_word(15)?],
            buffer,
            world,
        });
    }
    Ok(out)
}
