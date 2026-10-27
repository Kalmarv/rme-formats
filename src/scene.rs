//! Level meshes. A mesh quartet is an `.scp` (scene: placements, nodes, blob sections), an `.mlp` (the model table and
//! per-model metadata), an `.mmp` (CPU-side vertex and index data) and an `.mgp` (GPU-side vertex attributes) that share
//! a stem in a [`crate::pak::Pak`]. [`quartets`] finds them, [`decode`] reads one from decoded bytes and
//! [`decode_quartet`] from an archive.
//!
//! Positions and indices are checked against the stored bounds and counts, the submeshes must partition the index
//! list, and the spans of the files that are set aside undecoded are listed in [`Scene::unread`]. Offsets in diagnostics are
//! relative to the named decoded file. A model whose vertex variant is not supported is returned with
//! [`Model::refusal`] set and no geometry, while the other models decode.
use crate::attributes::{self, Attributes};
use crate::pak::Pak;
use crate::skin::{SourceBone, SourceSkinVertex};
use crate::{Affine, Result, element, invalid, offset, scp_tables, skin};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

/// A span of a quartet file the reader accounts for but does not decode, with why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unread {
    pub file: &'static str,
    pub offset: usize,
    pub bytes: usize,
    pub reason: &'static str,
}

/// A run of a model's indices drawn with one material.
#[derive(Clone, Debug)]
pub struct Submesh {
    pub indices: Range<usize>,
    /// The material id (resolve it with [`crate::library::MaterialLibrary::resolve`]).
    pub material: u32,
    /// Logical ATTR8..11 -> stored UV0..3; 255 retains the same-index source.
    pub uv_remap: [u8; 4],
}

/// One mesh of a quartet, in source coordinates.
#[derive(Clone, Debug)]
pub struct Model {
    pub id: u32,
    /// The stored axis-aligned bounds, `[min, max]`.
    pub bounds: [[f32; 3]; 2],
    pub positions: Vec<[f32; 3]>,
    pub format: u32,
    pub attributes: Attributes,
    pub processed: Option<Vec<SourceSkinVertex>>,
    pub processed_batches: usize,
    /// Triangle-list indices into `positions`.
    pub indices: Vec<u32>,
    /// Spans of `indices` that exactly partition them, each with its material.
    pub submeshes: Vec<Submesh>,
    /// MLP offset and reason; no geometry is fabricated for an unsupported model.
    pub refusal: Option<(usize, &'static str)>,
}

/// An instance of a model in the level: its transform and name hash.
#[derive(Clone, Debug)]
pub struct Placement {
    pub id: u32,
    pub transform: Affine,
    /// Offset of this 48-byte record in its SCP, all source words, and its name hash (+0x0c).
    pub offset: usize,
    pub words: [u32; 12],
    pub name: u32,
}

/// A decoded quartet.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The meshes, in the order of the `.mlp` resource table.
    pub models: Vec<Model>,
    /// The instances, in the order of the `.scp` placement table.
    pub placements: Vec<Placement>,
    /// The SCP's node table, world matrices and blob sections (spot chunks).
    pub tables: scp_tables::Tables,
    /// The skeleton of the processed skins, read from the SCP when any model has one.
    pub source_bones: Option<Vec<SourceBone>>,
    /// Spans the reader sets aside undecoded: metadata gaps of the MLP, unknown SCP tables, and the SCP blob sections
    /// and chain tail that no decoder reads. The spans of one file are disjoint. The SCP header and the fields of the
    /// tables that are decoded are not listed.
    pub unread: Vec<Unread>,
}

#[derive(Clone, Copy)]
pub(crate) struct Bytes<'a> {
    pub(crate) data: &'a [u8],
    file: &'static str,
    base: usize,
}
impl<'a> Bytes<'a> {
    pub(crate) fn error(self, at: usize, reason: &str) -> crate::Error {
        invalid(self.base.saturating_add(at), format!("{}: {reason}", self.file))
    }
    pub(crate) fn slice(self, at: usize, n: usize) -> Result<&'a [u8]> {
        let end = at
            .checked_add(n)
            .ok_or_else(|| self.error(at, "range overflow"))?;
        self.data
            .get(at..end)
            .ok_or_else(|| self.error(at, "range outside file/resource"))
    }
    fn sub(self, at: usize, n: usize) -> Result<Self> {
        Ok(Self {
            data: self.slice(at, n)?,
            base: offset(self.base, at)?,
            ..self
        })
    }
    pub(crate) fn w(self, at: usize) -> Result<u32> {
        let &[a, b, c, d] = self.slice(at, 4)? else {
            return Err(self.error(at, "word is not four bytes"));
        };
        Ok(u32::from_be_bytes([a, b, c, d]))
    }
    pub(crate) fn u(self, at: usize) -> Result<usize> {
        Ok(self.w(at)? as usize)
    }
    pub(crate) fn f(self, at: usize) -> Result<f32> {
        let n = f32::from_bits(self.w(at)?);
        if !n.is_finite() {
            return Err(self.error(at, "nonfinite float"));
        }
        Ok(n)
    }
    pub(crate) fn v3(self, at: usize) -> Result<[f32; 3]> {
        Ok([self.f(at)?, self.f(offset(at, 4)?)?, self.f(offset(at, 8)?)?])
    }
    pub(crate) fn require(self, at: usize, yes: bool, reason: &str) -> Result<()> {
        if yes { Ok(()) } else { Err(self.error(at, reason)) }
    }
    pub(crate) fn table(self, at: usize, count: usize, stride: usize) -> Result<()> {
        self.slice(
            at,
            count
                .checked_mul(stride)
                .ok_or_else(|| self.error(at, "table size overflow"))?,
        )?;
        Ok(())
    }
    pub(crate) fn matrix(self, start: usize) -> Result<Affine> {
        self.slice(start, 64)?;
        let mut cols = [[0.; 3]; 4];
        for (j, col) in cols.iter_mut().enumerate() {
            let col_at = element(start, j, 16, 0)?;
            *col = self.v3(col_at)?;
            let w_at = offset(col_at, 12)?;
            self.require(
                w_at,
                self.f(w_at)? == if j == 3 { 1. } else { 0. },
                "nonaffine matrix",
            )?;
        }
        Ok(Affine { cols })
    }
}

/// Validate disjoint coverage through EOF. Alignment holes must be zero; payload holes are errors.
fn coverage(b: Bytes<'_>, mut spans: Vec<Range<usize>>, padding: bool) -> Result<()> {
    spans.retain(|r| !r.is_empty());
    spans.sort_by_key(|r| r.start);
    let mut end = 0;
    for r in spans
        .into_iter()
        .chain(std::iter::once(b.data.len()..b.data.len()))
    {
        b.require(r.start, r.start >= end, "overlapping ranges")?;
        let gap = b.slice(end, r.start.saturating_sub(end))?;
        if !gap.is_empty() {
            b.require(end, padding, "unconsumed payload bytes")?;
            if let Some(i) = gap.iter().position(|&x| x != 0) {
                return Err(b.error(offset(end, i)?, "nonzero alignment/uncovered bytes"));
            }
        }
        b.slice(r.start, r.len())?;
        end = r.end;
    }
    Ok(())
}

/// Metadata has opaque saved-address/sentinel words beyond the decoded structures. Account for each gap.
fn unread_gaps(b: Bytes<'_>, mut spans: Vec<Range<usize>>) -> Result<Vec<Unread>> {
    spans.retain(|r| !r.is_empty());
    spans.sort_by_key(|r| r.start);
    let mut unread = Vec::new();
    let mut end = 0;
    for r in spans
        .into_iter()
        .chain(std::iter::once(b.data.len()..b.data.len()))
    {
        b.require(r.start, r.start >= end, "overlapping metadata structures")?;
        b.slice(r.start, r.len())?;
        if r.start > end {
            unread.push(Unread {
                file: b.file,
                offset: offset(b.base, end)?,
                bytes: r.start.saturating_sub(end),
                reason: "opaque mesh metadata outside decoded structures",
            });
        }
        end = r.end;
    }
    Ok(unread)
}

fn stream(b: Bytes<'_>, at: usize) -> Result<(usize, usize, Range<usize>)> {
    b.slice(at, 24)?;
    let count = b.u(at)?;
    let stride = b.u(offset(at, 8)?)?;
    let off = b.u(offset(at, 16)?)?;
    let size = b.u(offset(at, 20)?)?;
    b.require(
        offset(at, 20)?,
        count.checked_mul(stride) == Some(size),
        "stream size != count * stride",
    )?;
    Ok((count, stride, off..offset(off, size)?))
}

/// Decode one quartet in source coordinates. Unsupported format variants are refused individually.
/// Unknown attributes/tables are returned with exact byte spans rather than assigned guessed meanings.
pub fn decode(scp: &[u8], mlp: &[u8], mmp: &[u8], mgp: &[u8]) -> Result<Scene> {
    let s = Bytes {
        data: scp,
        file: "scp",
        base: 0,
    };
    let l = Bytes {
        data: mlp,
        file: "mlp",
        base: 0,
    };
    let m = Bytes {
        data: mmp,
        file: "mmp",
        base: 0,
    };
    let g = Bytes {
        data: mgp,
        file: "mgp",
        base: 0,
    };
    s.require(0, s.w(0)? == 0xbabf003c, "bad magic/version")?;
    s.slice(0, 0x48)?;
    l.require(0, l.w(0)? == 0xfafa0003, "bad magic/version")?;
    l.require(0x18, l.u(0x18)? == 28, "unsupported resource record size")?;
    let count = l.u(0x10)?;
    let data = l.u(0x14)?;
    l.table(28, count, 28)?;
    l.require(
        20,
        data >= element(28, count, 28, 0)?,
        "metadata base overlaps resource table",
    )?;
    l.slice(0, data)?;
    let mut spans = [std::iter::once(0..data).collect(), vec![], vec![]];
    let mut scene = Scene {
        models: vec![],
        placements: vec![],
        tables: scp_tables::Tables::default(),
        source_bones: None,
        unread: vec![],
    };
    let mut ids = BTreeSet::new();
    for i in 0..count {
        let record = element(28, i, 28, 0)?;
        let id = l.w(record)?;
        l.require(record, ids.insert(id), "duplicate model ID")?;
        let offsets = [
            offset(data, l.u(offset(record, 4)?)?)?,
            l.u(offset(record, 8)?)?,
            l.u(offset(record, 12)?)?,
        ];
        let sizes = [
            l.u(offset(record, 16)?)?,
            l.u(offset(record, 20)?)?,
            l.u(offset(record, 24)?)?,
        ];
        for ((span, &start), &size) in spans.iter_mut().zip(&offsets).zip(&sizes) {
            span.push(start..offset(start, size)?);
        }
        let [meta_at, cpu_at, gpu_at] = offsets;
        let [meta_size, cpu_size, gpu_size] = sizes;
        let b = l.sub(meta_at, meta_size)?;
        let cpu = m.sub(cpu_at, cpu_size)?;
        let gpu = g.sub(gpu_at, gpu_size)?;
        b.slice(0, 0xc0)?;
        b.require(0, b.w(0)? == 0xbaba0031, "bad mesh magic/version")?;
        b.require(0x80, b.w(0x80)? == id, "resource ID mismatch")?;
        b.require(
            16,
            b.matrix(16)? == Affine::IDENTITY,
            "unsupported nonidentity mesh-local matrix",
        )?;
        let bounds = [b.v3(0x60)?, b.v3(0x70)?];
        b.require(
            0x60,
            bounds[0].iter().zip(&bounds[1]).all(|(low, high)| low <= high),
            "inverted bounds",
        )?;
        let vp = b.u(0x90)?;
        let gp = b.u(0x94)?;
        let ip = b.u(0x98)?;
        let extra = b.u(0x9c)?;
        let sp = b.u(0xa0)?;
        let ns = b.u(0xa4)?;
        let (vc, stride, vr) = stream(b, vp)?;
        let embedded = extra != u32::MAX as usize;
        b.require(
            offset(vp, 8)?,
            stride == if embedded { 36 } else { 16 },
            "unsupported primary stride",
        )?;
        if embedded {
            b.slice(extra, 4)?;
        }
        b.slice(ip, 24)?;
        let ic = b.u(ip)?;
        let io = b.u(offset(ip, 8)?)?;
        let size = b.u(offset(ip, 12)?)?;
        b.require(offset(ip, 4)?, b.u(offset(ip, 4)?)? == 5, "unsupported topology")?;
        b.require(offset(ip, 16)?, b.u(offset(ip, 16)?)? == 1, "unsupported index type")?;
        b.require(offset(ip, 12)?, ic.checked_mul(2) == Some(size), "index size mismatch")?;
        b.require(ip, ic.is_multiple_of(3), "index count is not a triangle list")?;
        let ib = cpu.slice(io, size)?;
        let indices: Vec<u32> = ib
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u32::from(u16::from_be_bytes(pair)))
            .collect();
        for (j, &index) in indices.iter().enumerate() {
            cpu.require(element(io, j, 2, 0)?, (index as usize) < vc, "index out of range")?;
        }
        b.table(sp, ns, 96)?;
        let mut subs = Vec::new();
        let mut partition = Vec::new();
        for j in 0..ns {
            let p = element(sp, j, 96, 0)?;
            let first = b.u(offset(p, 32)?)?;
            let n = b.u(offset(p, 36)?)?;
            b.require(
                offset(p, 32)?,
                ic.checked_sub(first).is_some_and(|rest| n <= rest)
                    && first.is_multiple_of(3)
                    && n.is_multiple_of(3),
                "invalid submesh index span",
            )?;
            b.require(offset(p, 40)?, b.u(offset(p, 40)?)? == 0, "unsupported base vertex")?;
            let span = first..offset(first, n)?;
            partition.push(span.clone());
            let remap_at = offset(p, 56)?;
            subs.push(Submesh {
                indices: span,
                material: b.w(offset(p, 52)?)?,
                uv_remap: <[u8; 4]>::try_from(b.slice(remap_at, 4)?)
                    .map_err(|_| b.error(remap_at, "UV remap is not four bytes"))?,
            });
        }
        // A virtual byte per index allows the same exact-partition validation without allocating an index mask.
        partition.sort_by_key(|r| r.start);
        let mut end = 0;
        for r in partition {
            b.require(sp, r.start == end, "submesh partition gap or overlap")?;
            end = r.end;
        }
        b.require(sp, end == ic, "submesh partition does not reach index end")?;
        let mut positions = Vec::new();
        let format = b.w(0x8c)?;
        let mut attributes = Attributes::default();
        let mut processed = None;
        let mut processed_batches = 0;
        let unsupported_uv = subs
            .iter()
            .enumerate()
            .find_map(|(i, sub)| {
                sub.uv_remap
                    .iter()
                    .position(|&v| v > 3 && v != 255)
                    .map(|slot| (i, slot))
            })
            .map(|(i, slot)| -> Result<usize> { offset(offset(b.base, element(sp, i, 96, 56)?)?, slot) })
            .transpose()?;
        let refusal = if embedded {
            b.require(
                0x94,
                gp == u32::MAX as usize && gpu.data.is_empty(),
                "unsupported embedded MGP stream",
            )?;
            coverage(cpu, std::iter::once(io..offset(io, size)?).collect(), false)?;
            if format != 0x8007f || b.w(0xb4)? != 1 {
                Some((offset(b.base, 0x8c)?, "unsupported processed skin format/flags"))
            } else if let Some(at) = unsupported_uv {
                Some((at, "unsupported UV source selector"))
            } else {
                let bones = match scene.source_bones {
                    Some(ref bones) => bones,
                    None => scene.source_bones.insert(skin::skeleton(s)?),
                };
                let decoded = skin::decode(b, extra, vc, bones.len(), bounds)?;
                scene.unread.extend(unread_gaps(
                    b,
                    vec![
                        0..0xc0,
                        sp..element(sp, ns, 96, 0)?,
                        vp..offset(vp, 24)?,
                        ip..offset(ip, 24)?,
                        decoded.stream,
                    ],
                )?);
                positions = decoded.positions;
                attributes = decoded.attributes;
                processed = Some(decoded.vertices);
                processed_batches = decoded.batches;
                None
            }
        } else {
            cpu.slice(vr.start, vr.len())?;
            for j in 0..vc {
                let p_at = element(vr.start, j, 16, 0)?;
                let p = cpu.v3(p_at)?;
                cpu.require(
                    p_at,
                    p.iter()
                        .zip(&bounds[0])
                        .zip(&bounds[1])
                        .all(|((v, low), high)| *v >= low - 0.05 && *v <= high + 0.05),
                    "position outside stored AABB (0.05 source-unit tolerance)",
                )?;
                positions.push(p);
            }
            coverage(cpu, vec![vr.clone(), io..offset(io, size)?], false)?;
            let (gs, gr) = if gp == u32::MAX as usize {
                b.require(0x94, gpu.data.is_empty(), "missing MGP descriptor with payload")?;
                (0, 0..0)
            } else {
                let (gc, gs, gr) = stream(b, gp)?;
                b.require(gp, gc == vc, "MGP vertex count mismatch")?;
                coverage(gpu, vec![gr.clone()], false)?;
                (gs, gr)
            };
            let mut meta = vec![
                0..0xc0,
                sp..element(sp, ns, 96, 0)?,
                vp..offset(vp, 24)?,
                ip..offset(ip, 24)?,
            ];
            if gp != u32::MAX as usize {
                meta.push(gp..offset(gp, 24)?);
            }
            scene.unread.extend(unread_gaps(b, meta)?);
            if attributes::stride(format).is_none() {
                Some((offset(b.base, 0x8c)?, "unsupported rigid vertex format mask"))
            } else if let Some(at) = unsupported_uv {
                Some((at, "unsupported UV source selector"))
            } else {
                b.require(
                    0x94,
                    attributes::stride(format) == Some(gs),
                    "MGP stride differs from vertex format mask",
                )?;
                attributes = attributes::decode(
                    format,
                    vc,
                    cpu.sub(vr.start, vr.len())?,
                    gpu.sub(gr.start, gr.len())?,
                    gs,
                )?;
                None
            }
        };
        scene.models.push(Model {
            id,
            bounds,
            positions,
            format,
            attributes,
            processed,
            processed_batches,
            indices,
            submeshes: subs,
            refusal,
        });
    }
    let [meta_spans, cpu_spans, gpu_spans] = spans;
    coverage(l, meta_spans, true)?;
    coverage(m, cpu_spans, true)?;
    coverage(g, gpu_spans, true)?;
    let n = s.u(16)?;
    let off = s.u(20)?;
    s.table(off, n, 48)?;
    // The node and world-matrix tables and the blob chain are read by `scp_tables`.
    let (tables, tables_unread) = scp_tables::decode(scp)?;
    scene.tables = tables;
    scene.unread.extend(tables_unread);
    // Other native-loader tables remain opaque. Processed skins already retain the named bone table.
    for (field, stride) in [(0x20, 16), (0x28, 56), (0x30, 512)] {
        let count = s.u(field)?;
        let start = s.u(offset(field, 4)?)?;
        if count > 0 && !(field == 0x20 && scene.source_bones.is_some()) {
            s.table(start, count, stride)?;
            scene.unread.push(Unread {
                file: "scp",
                offset: start,
                bytes: element(0, count, stride, 0)?,
                reason: "unknown scene table",
            });
        }
    }
    for i in 0..n {
        let record = element(off, i, 48, 0)?;
        let id = s.w(offset(record, 8)?)?;
        s.require(offset(record, 8)?, ids.contains(&id), "placement references missing mesh")?;
        let mut words = [0u32; 12];
        for (j, word) in words.iter_mut().enumerate() {
            *word = s.w(element(record, j, 4, 0)?)?;
        }
        scene.placements.push(Placement {
            id,
            transform: s.matrix(s.u(record)?)?,
            offset: record,
            words,
            name: s.w(offset(record, 12)?)?,
        });
    }
    Ok(scene)
}

/// The quartets of `pak`: entries sharing a stem (the raw archive name before the extension) with extensions `.scp`,
/// `.mlp`, `.mmp` and `.mgp`, as indices into [`Pak::entries`] in that order. Stems are raw archive names, not host
/// filesystem paths. A repeated extension or an incomplete quartet is refused.
pub fn quartets(pak: &Pak) -> Result<BTreeMap<Vec<u8>, [usize; 4]>> {
    let mut groups: BTreeMap<Vec<u8>, [Option<usize>; 4]> = BTreeMap::new();
    for (i, e) in pak.entries().iter().enumerate() {
        if e.attributes == 1 {
            continue;
        }
        let Some(dot) = e.name.iter().rposition(|&c| c == b'.') else {
            continue;
        };
        let (stem, extension) = e.name.split_at(dot);
        let slot = match extension.get(1..) {
            Some(b"scp") => 0,
            Some(b"mlp") => 1,
            Some(b"mmp") => 2,
            Some(b"mgp") => 3,
            _ => continue,
        };
        let group = groups.entry(stem.to_vec()).or_insert([None; 4]);
        let cell = group
            .get_mut(slot)
            .ok_or_else(|| invalid(e.offset, "quartet extension slot"))?;
        if cell.replace(i).is_some() {
            return Err(invalid(e.offset, "duplicate quartet extension"));
        }
    }
    groups
        .into_iter()
        .map(|(name, g)| {
            if let [Some(a), Some(b), Some(c), Some(d)] = g {
                Ok((name, [a, b, c, d]))
            } else {
                Err(invalid(
                    0,
                    format!("incomplete quartet {:?}", String::from_utf8_lossy(&name)),
                ))
            }
        })
        .collect()
}

/// Decode the quartet whose `.scp`, `.mlp`, `.mmp` and `.mgp` entries are `entries` (from [`quartets`]).
pub fn decode_quartet(pak: &Pak, entries: [usize; 4]) -> Result<Scene> {
    let [a, b, c, d] = entries;
    decode(
        &pak.payload(a)?,
        &pak.payload(b)?,
        &pak.payload(c)?,
        &pak.payload(d)?,
    )
}
