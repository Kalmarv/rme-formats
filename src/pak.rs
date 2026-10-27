//! PAK v4 archives: a big-endian table of named entries in sections, each entry stored or CSTR-compressed
//! ([`crate::cstr`]).
//!
//! The whole table is checked when the archive is read: magic, endian tag and version, every entry's name hash (sdbm of
//! its name), attributes, alignment and payload span, and that the payloads tile the file exactly. Names keep their
//! original bytes (no path normalisation).
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "reader I/O: open the caller's archive"
)]
use crate::cstr;
use crate::{Result, invalid, offset};
use primase_foundations::bytes::{Endian::Big, Field, View};
use std::{borrow::Cow, ops::Range, path::Path};

/// Bytes before the first entry row, and the size of a row.
const HEADER: usize = 16;
const ROW: usize = 32;

/// One entry of the archive's table: a file, or a section marker (`attributes == 1`).
#[derive(Debug)]
pub struct Entry {
    /// The name as stored, case and separators included.
    pub name: Vec<u8>,
    /// sdbm hash of `name`, checked against it when the archive is read.
    pub hash: u32,
    /// Section index; a marker belongs to the section it opens.
    pub section: Option<usize>,
    /// `0` stored, `1` section marker, `2` CSTR-compressed.
    pub attributes: u32,
    /// Runtime allocation alignment, not file-offset alignment.
    pub alignment: u32,
    /// Offset of the stored payload in the archive file.
    pub offset: usize,
    /// Size of the decoded payload.
    pub size: usize,
    /// Zero for stored entries and markers; otherwise the encoded CSTR length.
    pub compressed_size: usize,
}

/// A named run of entries, opened by a section marker.
#[derive(Debug)]
pub struct Section {
    pub name: Vec<u8>,
    /// Includes this section's marker entry.
    pub entries: Range<usize>,
}

/// Owns archive bytes; metadata is checked at construction and payloads decode on request.
#[derive(Debug)]
pub struct Pak {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
    sections: Vec<Section>,
}

impl Pak {
    /// Read and check the archive at `path`. This is the only function in the crate that touches the file system.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_bytes(std::fs::read(path)?)
    }

    /// Check an archive held in memory: magic, endian tag, version, and every entry's hash, attributes, alignment and
    /// payload span. The payloads must tile the file exactly; a gap, an overlap or trailing bytes are refused with the offset.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let v = View::new(&bytes);
        if v.slice(0, 4).field("PAK magic")? != b"PAK\0" {
            return Err(invalid(0, "expected PAK"));
        }
        if v.u16(4, Big).field("PAK endian tag")? != 0xfeeb {
            return Err(invalid(4, "PAK endian tag"));
        }
        if v.u16(6, Big).field("PAK version")? != 4 {
            return Err(invalid(6, "unsupported PAK version"));
        }
        let count = v.u32(8, Big).field("entry count")? as usize;
        let strsize = v.u32(12, Big).field("string table size")? as usize;
        let strstart = count
            .checked_mul(ROW)
            .and_then(|n| n.checked_add(HEADER))
            .ok_or_else(|| invalid(8, "entry count overflow"))?;
        let strings = v.slice(strstart, strsize).field("string table")?;
        let mut pos = offset(strstart, strsize)?;
        let mut entries = Vec::new();
        let mut sections: Vec<Section> = Vec::new();
        for i in 0..count {
            // the table up to `strstart` was checked to fit, so no row start overflows
            let at = offset(HEADER, i.saturating_mul(ROW))?;
            let word =
                |k: usize, what: &'static str| -> Result<u32> { Ok(v.u32(offset(at, k)?, Big).field(what)?) };
            let hash = word(0, "entry hash")?;
            let name_off = word(4, "name offset")? as usize;
            let tail = strings
                .get(name_off..)
                .ok_or_else(|| invalid(at.saturating_add(4), "name offset outside string table"))?;
            let name = tail
                .iter()
                .position(|&b| b == 0)
                .and_then(|len| tail.get(..len))
                .ok_or_else(|| invalid(strstart.saturating_add(name_off), "unterminated name"))?;
            if primase_foundations::hash::sdbm(name) != hash {
                return Err(invalid(at, "name hash mismatch"));
            }
            let attributes = word(8, "attributes")?;
            if !matches!(attributes, 0..=2) {
                return Err(invalid(offset(at, 8)?, "unknown entry attributes"));
            }
            let alignment = word(12, "alignment")?;
            if !matches!(alignment, 0 | 16 | 64 | 128) {
                return Err(invalid(offset(at, 12)?, "unknown alignment"));
            }
            let payload_offset = word(16, "payload offset")? as usize;
            let size = word(20, "payload size")? as usize;
            let compressed_size = word(24, "compressed size")? as usize;
            if word(28, "runtime offset")? != 0 {
                return Err(invalid(offset(at, 28)?, "nonzero on-disc runtime offset"));
            }
            if payload_offset != pos {
                return Err(invalid(offset(at, 16)?, "payload gap or overlap"));
            }
            let stored_size = if attributes == 2 { compressed_size } else { size };
            v.slice(payload_offset, stored_size).field("entry payload")?;
            if (attributes == 2) != (compressed_size != 0) {
                return Err(invalid(
                    offset(at, 24)?,
                    "compressed attribute and compressed size disagree",
                ));
            }
            if attributes == 1 {
                if size != 0 || alignment != 0 {
                    return Err(invalid(
                        offset(at, 20)?,
                        "section marker has payload or alignment",
                    ));
                }
                if let Some(section) = sections.last_mut() {
                    section.entries.end = i;
                }
                sections.push(Section {
                    name: name.to_vec(),
                    entries: i..count,
                });
            }
            entries.push(Entry {
                name: name.to_vec(),
                hash,
                section: sections.len().checked_sub(1),
                attributes,
                alignment,
                offset: payload_offset,
                size,
                compressed_size,
            });
            pos = offset(pos, stored_size)?;
        }
        if pos != bytes.len() {
            return Err(invalid(pos, "trailing bytes after PAK payloads"));
        }
        Ok(Self {
            bytes,
            entries,
            sections,
        })
    }

    /// Every entry in table order, section markers included.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// The sections in table order.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Raw encoded bytes (a marker has an empty payload).
    pub fn encoded(&self, index: usize) -> Result<&[u8]> {
        self.stored(index).map(|(_, bytes)| bytes)
    }

    /// The entry and its stored bytes.
    fn stored(&self, index: usize) -> Result<(&Entry, &[u8])> {
        let e = self
            .entries
            .get(index)
            .ok_or_else(|| invalid(8, "entry index out of bounds"))?;
        let stored_size = if e.attributes == 2 {
            e.compressed_size
        } else {
            e.size
        };
        // `from_bytes` checked this span, so it only fails for a `Pak` built some other way
        let bytes = View::new(&self.bytes)
            .slice(e.offset, stored_size)
            .field("entry payload")?;
        Ok((e, bytes))
    }

    /// Stored entries borrow the archive; compressed entries allocate only when requested.
    /// Compression errors identify the entry's absolute payload offset and the stream-relative error.
    pub fn payload(&self, index: usize) -> Result<Cow<'_, [u8]>> {
        let (e, bytes) = self.stored(index)?;
        if e.attributes == 2 {
            Ok(Cow::Owned(
                cstr::decode(bytes, e.size)
                    .map_err(|err| invalid(e.offset, err))?
                    .bytes,
            ))
        } else {
            Ok(Cow::Borrowed(bytes))
        }
    }
}
