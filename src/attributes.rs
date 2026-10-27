//! The packed vertex attributes of a rigid mesh: RSX CMP normals, half-float and byte inputs, selected by the model's
//! vertex format mask. The layout follows the table at 0x1445920 in the game's executable (the variant selected by mask
//! bit 0x80000). No tangent sign is inferred.
use crate::scene::Bytes;
use crate::{Result, element, offset};
use std::collections::BTreeMap;

/// Decoded source lanes, padded with zero to four components. Values retain their source encoding's meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct VertexInput {
    /// How many of the four lanes the source encoding stores; the rest are zero.
    pub lanes: u8,
    /// One entry per vertex, in vertex order.
    pub values: Vec<[f32; 4]>,
}

/// The per-vertex data of a model besides its positions, decoded according to the model's vertex format mask.
#[derive(Clone, Debug, Default)]
pub struct Attributes {
    /// Source normals: exact RSX CMP for rigid meshes, authored float3 for processed skins. No repair.
    pub normals: Vec<[f32; 3]>,
    /// Vertex colours (RGBA, 0 to 1), when the format has them.
    pub colors: Option<Vec<[f32; 4]>>,
    /// The four stored texture-coordinate sets, where present.
    pub uvs: [Option<Vec<[f32; 2]>>; 4],
    /// Every decoded input by name (`stored_uv0`, `attr12`, ...), including those whose role is not known.
    pub raw: BTreeMap<String, VertexInput>,
    /// How many normals are the zero vector (accepted as stored, not repaired).
    pub zero_normals: usize,
}

/// RSX sign extends 11:11:10 lanes after shifting to i16 and divides by 32767.
pub fn cmp(word: u32) -> [f32; 3] {
    // Each lane is moved to the top of the word and shifted back arithmetically: that sign-extends it and leaves it
    // scaled to i16 (the 11-bit lanes by 2^5, the 10-bit lane by 2^6).
    let lane = |top_aligned: u32| (top_aligned as i32 >> 16) as f32 / 32767.;
    [
        lane(word << 21),
        lane((word >> 11) << 21),
        lane((word >> 22) << 22),
    ]
}

/// Byte size from the native declaration entries; unsupported flags/entries refuse this model.
pub(crate) fn stride(mask: u32) -> Option<usize> {
    if mask & !0x807ff != 0 || mask & 0x80005 != 0x80005 {
        return None;
    }
    Some(
        (1..=10)
            .filter(|&bit| bit != 2 && mask & (1 << bit) != 0)
            .map(|bit| if bit >= 9 { 8 } else { 4 })
            .sum(),
    )
}

pub(crate) fn decode(
    mask: u32,
    count: usize,
    cpu: Bytes<'_>,
    gpu: Bytes<'_>,
    gpu_stride: usize,
) -> Result<Attributes> {
    let mut a = Attributes::default();
    for (i, row) in cpu.data.as_chunks::<16>().0.iter().enumerate() {
        let [.., w0, w1, w2, w3] = *row;
        let n = cmp(u32::from_be_bytes([w0, w1, w2, w3]));
        let len = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        cpu.require(
            element(0, i, 16, 12)?,
            len == 0. || (len - 1.).abs() <= 0.005,
            "nonunit CMP normal (0.005 tolerance)",
        )?;
        a.zero_normals = a.zero_normals.saturating_add(usize::from(len == 0.));
        a.normals.push(n);
    }
    let mut column = 0usize;
    for bit in (1u32..=10).filter(|&b| b != 2 && mask & (1 << b) != 0) {
        let mut values = Vec::with_capacity(count);
        let lanes: u8 = if bit == 1 || bit >= 9 {
            4
        } else if bit >= 5 {
            2
        } else {
            3
        };
        for i in 0..count {
            let p = element(column, i, gpu_stride, 0)?;
            let mut v = [0.; 4];
            if bit == 1 {
                for (dst, &src) in v.iter_mut().zip(gpu.slice(p, 4)?) {
                    *dst = f32::from(src) / 255.;
                }
            } else if bit <= 4 {
                let [x, y, z] = cmp(gpu.w(p)?);
                v = [x, y, z, 0.];
            } else {
                for (k, dst) in v.iter_mut().take(usize::from(lanes)).enumerate() {
                    let lane_at = element(p, k, 2, 0)?;
                    let bytes = gpu.slice(lane_at, 2)?;
                    let &[hi, lo] = bytes else {
                        return Err(gpu.error(lane_at, "half-float lane is not two bytes"));
                    };
                    *dst = half::f16::from_bits(u16::from_be_bytes([hi, lo])).to_f32();
                    gpu.require(lane_at, dst.is_finite(), "nonfinite half vertex input")?;
                }
            }
            values.push(v);
        }
        if bit == 1 {
            a.colors = Some(values);
        } else {
            let uv_set = match bit {
                5 => Some(0),
                6 => Some(1),
                7 => Some(2),
                8 => Some(3),
                _ => None,
            };
            let name = match (bit, uv_set) {
                (_, Some(set)) => format!("stored_uv{set}"),
                (3, _) => "attr15".to_owned(),
                (4, _) => "attr14".to_owned(),
                (9, _) => "attr12".to_owned(),
                _ => "attr13".to_owned(),
            };
            if let Some(slot) = uv_set.and_then(|set| a.uvs.get_mut(set)) {
                *slot = Some(values.iter().map(|&[u, v, ..]| [u, v]).collect());
            }
            a.raw.insert(name, VertexInput { lanes, values });
        }
        column = offset(column, if bit >= 9 { 8 } else { 4 })?;
    }
    Ok(a)
}
