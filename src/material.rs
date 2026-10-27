//! MBP/MDP bank records. All offsets are BE and MDP pointers are relative to +0x40.
//! Opaque header/state fields are explicit data, not interpreted enums. New structural
//! layouts, encoding words, sentinels, gaps, overlaps and nonzero padding are errors.
use crate::{Result, element, invalid, offset};
use primase_foundations::bytes::{Endian::Big, Field, View};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// IDs are local to a bank: ID zero occurs in all three banks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MaterialKey {
    pub bank: String,
    pub id: u32,
}

/// A constant of a material: four floats under a name hash.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MaterialParameter {
    pub hash: u32,
    /// Only named by an exact sdbm join against this material's fragment program.
    pub name: Option<String>,
    /// Original descriptor word. The high bits' semantics remain unknown.
    pub source_encoding: u32,
    /// Four stored f32 values, including unused lanes, in source order.
    pub value: [f32; 4],
}

/// A texture binding of a material: its stored state and the texture path it names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MaterialSampler {
    /// Descriptor ordinal; no claim that this is a hardware texture unit.
    pub slot: u32,
    pub hash: u32,
    /// Exact sdbm join against an engine string; not Cg reflection or inferred use.
    pub name: Option<String>,
    pub name_source: Option<SamplerNameSource>,
    /// Empty is a source empty binding, not a generated default texture.
    pub texture: String,
    /// Descriptor +0, +4, +8, +12, +20. State semantics are not yet decoded.
    pub unknown_state: [u32; 5],
}

/// How a sampler's name was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SamplerNameSource {
    /// The sampler's hash equals the sdbm of a string in the game's executable.
    ExecutableStringSdbm,
}

/// A bank material, retained as authored: no shading conversion and no render-state guesses.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShredMaterial {
    pub key: MaterialKey,
    /// Header +0x1c, +0x28, +0x2c, +0x3c, respectively.
    pub unknown_header: [u32; 4],
    pub vertex_shader: String,
    pub fragment_shader: String,
    pub parameters: Vec<MaterialParameter>,
    pub samplers: Vec<MaterialSampler>,
}

/// Reflected names from the bound fragment program, indexed by the source hash.
pub type UniformNames = BTreeMap<u32, String>;

/// The four sampler names the shipped executable carries as NUL-terminated strings, found by their exact sdbm
/// hashes. A match identifies a name only, not a texture unit or a shading role. No compiled Cg parameter table
/// supplied these names.
pub fn sampler_name(hash: u32) -> Option<&'static str> {
    match hash {
        0x95591c40 => Some("Color_Map"),
        0xd79d6be4 => Some("Normal_Map"),
        0xffc34d98 => Some("Spec_Map"),
        0x2538db2e => Some("Effect_Map"),
        _ => None,
    }
}

fn word(b: &[u8], at: usize) -> Result<u32> {
    View::new(b)
        .u32(at, Big)
        .field("material word")
        .map_err(Into::into)
}
fn expect(b: &[u8], at: usize, value: u32) -> Result<()> {
    if word(b, at)? != value {
        return Err(invalid(at, format!("expected {value:#x}")));
    }
    Ok(())
}
fn pointer(b: &[u8], at: usize, expected: usize) -> Result<()> {
    // MDP pointers are relative to +0x40
    let stored = usize::try_from(word(b, at)?).ok().and_then(|p| p.checked_add(64));
    if stored != Some(expected) {
        return Err(invalid(at, "material pointer gap, overlap or unexpected layout"));
    }
    Ok(())
}
fn string(b: &[u8], pos: &mut usize) -> Result<String> {
    let tail = b
        .get(*pos..)
        .ok_or_else(|| invalid(*pos, "string outside record"))?;
    let n = tail
        .iter()
        .position(|&x| x == 0)
        .ok_or_else(|| invalid(*pos, "unterminated string"))?;
    let text = tail
        .get(..n)
        .filter(|text| text.is_ascii())
        .ok_or_else(|| invalid(*pos, "non-ASCII source name"))?;
    let out = String::from_utf8(text.to_vec()).map_err(|e| invalid(*pos, e))?;
    *pos = offset(*pos, n)?;
    *pos = offset(*pos, 1)?;
    Ok(out)
}

/// Parse one complete record. `names` must come from this record's bound PSP, not
/// a global dictionary: the same name hash in another program is not proof of binding.
pub fn record(bank: &str, b: &[u8], names: &UniformNames) -> Result<ShredMaterial> {
    for (at, value) in [
        (0, 0xaaaa0003),
        (4, 64),
        (8, 0x00400000),
        (12, 0xbeefface),
        (0x14, 0xffff000e),
        (0x18, u32::MAX),
        (0x20, u32::MAX),
        (0x24, u32::MAX),
        (0x30, u32::MAX),
        (0x34, u32::MAX),
        (0x38, 0),
        (0x40, u32::MAX),
    ] {
        expect(b, at, value)?;
    }
    let mut pos = 96;
    pointer(b, 0x44, pos)?;
    let vertex_shader = string(b, &mut pos)?;
    pointer(b, 0x48, pos)?;
    let fragment_shader = string(b, &mut pos)?;
    pointer(b, 0x50, pos)?;
    let np = word(b, 0x4c)? as usize;
    let nt = word(b, 0x54)? as usize;
    // Bound counts before arithmetic or allocation (also on 32-bit hosts).
    if np > b.len() / 32 || nt > b.len() / 32 {
        return Err(invalid(0x4c, "material counts exceed record"));
    }
    let descriptors = pos;
    pos = element(pos, np, 16, 0)?;
    let mut parameters = Vec::new();
    for i in 0..np {
        let descriptor = element(descriptors, i, 16, 0)?;
        let hash = word(b, descriptor)?;
        let encoding = word(b, offset(descriptor, 4)?)?;
        if !matches!(encoding, 0x000100ad | 0x020100ad) {
            return Err(invalid(
                offset(descriptor, 4)?,
                "unknown material parameter encoding",
            ));
        }
        expect(b, offset(descriptor, 8)?, u32::MAX)?;
        pointer(b, offset(descriptor, 12)?, pos)?;
        let mut value = [0.; 4];
        for (j, v) in value.iter_mut().enumerate() {
            let value_at = element(pos, j, 4, 0)?;
            *v = View::new(b).f32(value_at, Big).field("material value")?;
            if !v.is_finite() {
                return Err(invalid(value_at, "non-finite material value"));
            }
        }
        pos = offset(pos, 16)?;
        parameters.push(MaterialParameter {
            hash,
            name: names.get(&hash).cloned(),
            source_encoding: encoding,
            value,
        });
    }
    pointer(b, 0x58, pos)?;
    let mut samplers = Vec::new();
    for i in 0..nt {
        let sampler = pos;
        expect(b, offset(sampler, 24)?, 0xffff000e)?;
        let hash = word(b, offset(sampler, 16)?)?;
        let name = sampler_name(hash);
        samplers.push(MaterialSampler {
            slot: u32::try_from(i).map_err(|_| invalid(sampler, "sampler slot outside u32"))?,
            hash,
            name: name.map(str::to_owned),
            name_source: name.map(|_| SamplerNameSource::ExecutableStringSdbm),
            texture: String::new(),
            unknown_state: [
                word(b, sampler)?,
                word(b, offset(sampler, 4)?)?,
                word(b, offset(sampler, 8)?)?,
                word(b, offset(sampler, 12)?)?,
                word(b, offset(sampler, 20)?)?,
            ],
        });
        pos = offset(sampler, 28)?;
    }
    pointer(b, 0x5c, pos)?;
    let paths = pos;
    pos = element(pos, nt, 4, 0)?;
    for (i, sampler) in samplers.iter_mut().enumerate() {
        pointer(b, element(paths, i, 4, 0)?, pos)?;
        sampler.texture = string(b, &mut pos)?;
    }
    let aligned = offset(pos, 15)? & !15;
    if b.len() != aligned {
        return Err(invalid(pos, "record must end at next 16-byte boundary"));
    }
    let padding = b
        .get(pos..)
        .ok_or_else(|| invalid(pos, "record padding outside record"))?;
    if let Some(i) = padding.iter().position(|&x| x != 0) {
        return Err(invalid(offset(pos, i)?, "nonzero record padding"));
    }
    Ok(ShredMaterial {
        key: MaterialKey {
            bank: bank.into(),
            id: word(b, 0x10)?,
        },
        unknown_header: [word(b, 0x1c)?, word(b, 0x28)?, word(b, 0x2c)?, word(b, 0x3c)?],
        vertex_shader,
        fragment_shader,
        parameters,
        samplers,
    })
}

/// Parse the full index and payload. The callback resolves only the requested PSP
/// stem; return an empty table if absent. Names and empty bindings are never invented.
pub fn bank(
    name: &str,
    index: &[u8],
    data: &[u8],
    mut names: impl FnMut(&str) -> Result<UniformNames>,
) -> Result<Vec<ShredMaterial>> {
    for (at, value) in [(0, 0xfadd0001), (8, 0x00180000), (12, 0xbeefface), (20, 24)] {
        expect(index, at, value)?;
    }
    if word(index, 4)? as usize != index.len() {
        return Err(invalid(4, "MBP size"));
    }
    let n = word(index, 16)? as usize;
    if n > index.len() / 12 || element(24, n, 12, 0)? != index.len() {
        return Err(invalid(16, "MBP table does not end at EOF"));
    }
    let mut out = Vec::new();
    let mut pos = 0;
    let mut ids = BTreeSet::new();
    for i in 0..n {
        let row = element(24, i, 12, 0)?;
        let id = word(index, row)?;
        if !ids.insert(id) {
            return Err(invalid(row, "duplicate material ID within bank"));
        }
        if word(index, offset(row, 4)?)? as usize != pos {
            return Err(invalid(offset(row, 4)?, "MDP slices must tile"));
        }
        let packed = word(index, offset(row, 8)?)?;
        if packed & 0xffc00000 != 0x00400000 {
            return Err(invalid(offset(row, 8)?, "unknown packed size flags"));
        }
        let size = (packed & 0x003fffff) as usize;
        let b = View::new(data).slice(pos, size).field("MDP record")?;
        let shader_at = offset(pos, 0x48)?;
        let mut s =
            offset(word(b, 0x48)? as usize, 64).map_err(|_| invalid(shader_at, "shader offset overflow"))?;
        let shader = string(b, &mut s)?;
        let m = record(name, b, &names(&shader)?)
            .map_err(|e| invalid(pos, format!("bank {name} id {id}: {e}")))?;
        if m.key.id != id {
            return Err(invalid(offset(pos, 16)?, "MBP/MDP ID mismatch"));
        }
        out.push(m);
        pos = offset(pos, size)?;
    }
    if pos != data.len() {
        return Err(invalid(pos, "MDP trailing bytes"));
    }
    Ok(out)
}

/// Read only CgBinaryProgram's named-parameter table from every 64-byte-aligned
/// fragment permutation. This is reflection, not an instruction/shader decoder.
pub fn fragment_uniform_names(b: &[u8]) -> Result<UniformNames> {
    let mut out = UniformNames::new();
    let mut pos = 0;
    while pos < b.len() {
        let tail = b
            .get(pos..)
            .ok_or_else(|| invalid(pos, "Cg program outside the payload"))?;
        expect(tail, 0, 0x1b5c)?;
        expect(tail, 4, 6)?;
        let size = word(tail, 8)? as usize;
        if size < 32 {
            return Err(invalid(offset(pos, 8)?, "Cg program too short"));
        }
        let program = View::new(b).slice(pos, size).field("Cg program")?;
        let count = word(program, 12)? as usize;
        let table = word(program, 16)? as usize;
        if count > size / 48 {
            return Err(invalid(offset(pos, 12)?, "Cg parameter count"));
        }
        View::new(program)
            .slice(table, element(0, count, 48, 0)?)
            .field("Cg parameters")?;
        for i in 0..count {
            let entry = element(table, i, 48, 0)?;
            // Cg variability 4102 is uniform; other interface classes cannot name
            // a material constant, even if their string happens to hash the same.
            if word(program, offset(entry, 8)?)? != 4102 {
                continue;
            }
            let mut name_at = word(program, offset(entry, 16)?)? as usize;
            if name_at == 0 {
                continue;
            }
            let name = string(program, &mut name_at)?;
            if name.is_empty() {
                continue;
            }
            for n in [name.as_str(), name.split('[').next().unwrap_or(&name)] {
                let h = primase_foundations::hash::sdbm(n.as_bytes());
                if let Some(old) = out.insert(h, n.into())
                    && old != n
                {
                    return Err(invalid(offset(pos, entry)?, "Cg parameter name hash collision"));
                }
            }
        }
        let size_end = offset(pos, size)?;
        let next = offset(size_end, 63)? & !63;
        let padding = View::new(b)
            .slice(size_end, next.saturating_sub(size_end))
            .field("Cg padding")?;
        if let Some(i) = padding.iter().position(|&x| x != 0) {
            return Err(invalid(offset(size_end, i)?, "Cg nonzero padding"));
        }
        pos = next;
    }
    Ok(out)
}
