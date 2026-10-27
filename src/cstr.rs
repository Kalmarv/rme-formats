//! CSTR v1, the framing of one compressed [`crate::pak`] entry: a header (magic, endian tag, version, block count,
//! total size) and blocks with 16-bit sizes, each stored as is or as RFC 1951 raw DEFLATE. The header, every block
//! size and the total are checked, and nothing may follow the last block.
use crate::{Result, invalid, offset};
use primase_foundations::bytes::{Endian::Big, Field, View};
use primase_foundations::compress::{Decompressor, Deflate};

/// Decoded bytes and block counts (including stored blocks).
#[derive(Debug)]
pub struct Decoded {
    /// The decoded stream: every block's payload, concatenated, exactly the entry's expected size.
    pub bytes: Vec<u8>,
    /// Number of blocks in the stream, as the CSTR header counts them (stored blocks included).
    pub blocks: usize,
    /// How many of those blocks are stored rather than DEFLATE-compressed.
    pub stored_blocks: usize,
}

/// Decode exactly one stream, checking its total against the enclosing entry size.
/// Errors use stream-relative offsets.
pub fn decode(bytes: &[u8], expected: usize) -> Result<Decoded> {
    let v = View::new(bytes);
    if v.slice(0, 4).field("CSTR magic")? != b"CSTR" {
        return Err(invalid(0, "expected CSTR"));
    }
    for (at, want, what) in [
        (4, 0xfeed, "CSTR endian tag"),
        (6, 1, "CSTR version"),
        (10, 0xf32f, "CSTR threshold"),
    ] {
        if v.u16(at, Big).field(what)? != want {
            return Err(invalid(at, what));
        }
    }
    let count = usize::from(v.u16(8, Big).field("block count")?);
    if v.u32(12, Big).field("stream size")? as usize != expected {
        return Err(invalid(12, "stream/entry size mismatch"));
    }
    let mut out = Vec::new();
    let mut pos = 16;
    let mut stored = 0usize;
    for _ in 0..count {
        let csize = usize::from(v.u16(pos, Big).field("block compressed size")?);
        let size_at = offset(pos, 2)?;
        let size = usize::from(v.u16(size_at, Big).field("block output size")?);
        if out.len().checked_add(size).is_none_or(|n| n > expected) {
            return Err(invalid(size_at, "block exceeds stream output size"));
        }
        pos = offset(pos, 4)?;
        let payload = v.slice(pos, csize).field("block payload")?;
        out.try_reserve(size).map_err(|e| invalid(pos, e))?;
        if csize == size {
            stored = stored.saturating_add(1);
            out.extend_from_slice(payload);
        } else {
            let decoded = Deflate.decompress(payload, size).map_err(|e| invalid(pos, e))?;
            out.extend_from_slice(&decoded);
        }
        pos = offset(pos, csize)?;
    }
    if pos != bytes.len() {
        return Err(invalid(pos, "trailing bytes after CSTR blocks"));
    }
    if out.len() != expected {
        return Err(invalid(pos, "block output sum differs from stream size"));
    }
    Ok(Decoded {
        bytes: out,
        blocks: count,
        stored_blocks: stored,
    })
}
