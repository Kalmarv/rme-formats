//! Processed skin meshes: batches of 64-byte vertex records with bone palettes and 8-bit weights, as the game's
//! preprocessing step writes them (the functions at 0x430e38 and 0x430b28 in the executable). The decoding is internal
//! to [`crate::scene`]; its results appear as [`crate::scene::Model::processed`] and [`SourceBone`].
use crate::attributes::{Attributes, VertexInput};
use crate::scene::Bytes;
use crate::{Affine, Result, element, offset};
use std::ops::Range;

/// One bone of a mesh's source skeleton, as its SCP stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceBone {
    /// The bone's two leading words.
    pub words: [u32; 2],
    /// The bone's name without its terminator.
    pub name: Vec<u8>,
    /// Original SCP matrix; its space and the two words' runtime roles remain unproved.
    pub matrix: Affine,
}

/// One source vertex's palette and quantized weights. 255 is an inactive bone, with zero weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSkinVertex {
    /// The four palette bones; 255 is an inactive slot.
    pub bones: [u8; 4],
    /// The four 8-bit weights, which sum to 255.
    pub weights: [u8; 4],
    /// Original float bits, including stock NaNs.
    pub normal_bits: [u32; 3],
    /// Source +28, whose output colour role is unproved.
    pub extra: u32,
    /// The eight padding bytes of the record as stored.
    pub padding: [u8; 8],
}

pub(crate) struct Decoded {
    pub positions: Vec<[f32; 3]>,
    pub attributes: Attributes,
    pub vertices: Vec<SourceSkinVertex>,
    pub stream: Range<usize>,
    pub batches: usize,
}

pub(crate) fn skeleton(scp: Bytes<'_>) -> Result<Vec<SourceBone>> {
    let count = scp.u(0x20)?;
    let table = scp.u(0x24)?;
    scp.require(0x20, count > 0, "processed skin has no source skeleton")?;
    scp.table(table, count, 16)?;
    let mut bones = Vec::with_capacity(count);
    for i in 0..count {
        let record = element(table, i, 16, 0)?;
        let name = scp.u(offset(record, 8)?)?;
        let tail = scp.slice(name, scp.data.len().saturating_sub(name))?;
        let name_bytes = tail
            .iter()
            .position(|&x| x == 0)
            .and_then(|length| tail.get(..length))
            .ok_or_else(|| scp.error(name, "unterminated source bone name"))?;
        bones.push(SourceBone {
            words: [scp.w(record)?, scp.w(offset(record, 4)?)?],
            name: name_bytes.to_vec(),
            matrix: scp.matrix(scp.u(offset(record, 12)?)?)?,
        });
    }
    Ok(bones)
}

fn short(b: Bytes<'_>, at: usize) -> Result<usize> {
    let &[hi, lo] = b.slice(at, 2)? else {
        return Err(b.error(at, "short is not two bytes"));
    };
    Ok(usize::from(u16::from_be_bytes([hi, lo])))
}

fn half_pair(first: [u8; 2], second: [u8; 2]) -> [f32; 2] {
    [first, second].map(|bits| half::f16::from_bits(u16::from_be_bytes(bits)).to_f32())
}

pub(crate) fn decode(
    b: Bytes<'_>,
    ep: usize,
    count: usize,
    bones: usize,
    bounds: [[f32; 3]; 2],
) -> Result<Decoded> {
    let size = b.u(ep)?;
    b.slice(ep, size)?;
    b.require(
        ep,
        size >= 128 && count <= size / 64,
        "invalid processed skin size/count",
    )?;
    let end = offset(ep, size)?;
    b.require(
        offset(ep, 4)?,
        b.slice(offset(ep, 4)?, 60)?
            .as_chunks::<4>()
            .0
            .iter()
            .all(|w| *w == [0xaa, 0xbb, 0xcc, 0xdd]),
        "processed skin envelope padding",
    )?;
    let mut result = Decoded {
        positions: Vec::with_capacity(count),
        attributes: Attributes::default(),
        vertices: Vec::with_capacity(count),
        stream: ep..end,
        batches: 0,
    };
    let mut basis = Vec::with_capacity(count);
    let mut uvs = [Vec::with_capacity(count), Vec::with_capacity(count)];
    let mut cursor = offset(ep, 64)?;
    while result.vertices.len() < count {
        let header = b.slice(cursor, 64)?;
        result.batches = result.batches.saturating_add(1);
        b.require(
            cursor,
            offset(cursor, 64)? <= end,
            "processed batch exceeds stream",
        )?;
        let &[p0, p1, p2, p3, _, _, _, _, ref padding @ ..] = header else {
            return Err(b.error(cursor, "processed batch header is not 64 bytes"));
        };
        let palette = [p0, p1, p2, p3];
        let n = short(b, offset(cursor, 4)?)?;
        let active = short(b, offset(cursor, 6)?)?;
        b.require(
            offset(cursor, 4)?,
            n > 0
                && count
                    .checked_sub(result.vertices.len())
                    .is_some_and(|rest| n <= rest),
            "processed batch vertex count",
        )?;
        b.require(
            offset(cursor, 6)?,
            (1..=4).contains(&active),
            "processed batch influence count",
        )?;
        let (used, unused) = palette
            .split_at_checked(active)
            .ok_or_else(|| b.error(cursor, "processed batch influence count"))?;
        b.require(
            cursor,
            used.iter().all(|&i| i != 255 && usize::from(i) < bones) && unused.iter().all(|&i| i == 255),
            "processed batch bone palette",
        )?;
        b.require(
            offset(cursor, 8)?,
            padding
                .as_chunks::<4>()
                .0
                .iter()
                .all(|w| *w == [0xdd, 0xcc, 0xbb, 0xaa]),
            "processed batch padding",
        )?;
        cursor = offset(cursor, 64)?;
        for _ in 0..n {
            let bytes: [u8; 64] = b
                .slice(cursor, 64)?
                .try_into()
                .map_err(|_| b.error(cursor, "processed vertex is not 64 bytes"))?;
            b.require(
                cursor,
                offset(cursor, 64)? <= end,
                "processed vertex exceeds stream",
            )?;
            let weights = <[u8; 4]>::try_from(&bytes[12..16]).map_err(|_| b.error(cursor, "weights"))?;
            b.require(
                offset(cursor, 12)?,
                weights.iter().map(|&w| u16::from(w)).sum::<u16>() == 255
                    && weights
                        .get(active..)
                        .is_some_and(|rest| rest.iter().all(|&w| w == 0)),
                "processed weights must sum to 255 with inactive lanes zero",
            )?;
            let normal_bits = [
                b.w(offset(cursor, 16)?)?,
                b.w(offset(cursor, 20)?)?,
                b.w(offset(cursor, 24)?)?,
            ];
            let normal = normal_bits.map(f32::from_bits);
            if normal.iter().all(|n| n.is_finite()) {
                let length = normal.iter().map(|n| n * n).sum::<f32>().sqrt();
                b.require(
                    offset(cursor, 16)?,
                    length == 0. || (length - 1.).abs() <= 0.005,
                    "nonunit processed normal (0.005 tolerance)",
                )?;
                result.attributes.zero_normals = result
                    .attributes
                    .zero_normals
                    .saturating_add(usize::from(length == 0.));
            }
            let position = b.v3(cursor)?;
            b.require(
                cursor,
                position
                    .iter()
                    .zip(&bounds[0])
                    .zip(&bounds[1])
                    .all(|((v, low), high)| *v >= low - 0.05 && *v <= high + 0.05),
                "processed position outside stored AABB",
            )?;
            result.positions.push(position);
            result.attributes.normals.push(normal);
            basis.push([
                b.f(offset(cursor, 32)?)?,
                b.f(offset(cursor, 36)?)?,
                b.f(offset(cursor, 40)?)?,
                b.f(offset(cursor, 44)?)?,
            ]);
            let (uv_bytes, _) = bytes[48..56].as_chunks::<4>();
            for (set, (uv, &[a, c, d, e])) in uvs.iter_mut().zip(uv_bytes).enumerate() {
                let value = half_pair([a, c], [d, e]);
                b.require(
                    element(cursor, set, 4, 48)?,
                    value.iter().all(|v| v.is_finite()),
                    "nonfinite processed UV",
                )?;
                uv.push(value);
            }
            result.vertices.push(SourceSkinVertex {
                bones: palette,
                weights,
                normal_bits,
                extra: b.w(offset(cursor, 28)?)?,
                padding: <[u8; 8]>::try_from(&bytes[56..]).map_err(|_| b.error(cursor, "padding"))?,
            });
            cursor = offset(cursor, 64)?;
        }
    }
    b.slice(cursor, 64)?;
    b.require(
        cursor,
        offset(cursor, 64)? == end
            && short(b, offset(cursor, 4)?)? == 0
            && short(b, offset(cursor, 6)?)? == 0,
        "processed stream terminal/size mismatch",
    )?;
    result.attributes.raw.insert(
        "skin_basis".into(),
        VertexInput {
            lanes: 4,
            values: basis,
        },
    );
    for (set, uv) in uvs.into_iter().enumerate() {
        result.attributes.raw.insert(
            format!("stored_uv{set}"),
            VertexInput {
                lanes: 2,
                values: uv.iter().map(|&[u, v]| [u, v, 0., 0.]).collect(),
            },
        );
        if let Some(slot) = result.attributes.uvs.get_mut(set) {
            *slot = Some(uv);
        }
    }
    Ok(result)
}
