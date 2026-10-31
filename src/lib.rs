//! Readers for the game data of Tony Hawk's SHRED (PS3, 2010), which runs on Robomodo's RME engine.
//!
//! It is written against the files of the PS3 disc (the `PS3_GAME/USRDIR` folder; 32-bit big-endian pointers).
//!
//! Bytes in, plain records out: archives, level meshes, materials, textures, the game's SQLite database, a level's
//! Lua parameter script and animation reference rigs. Nothing here loads a game, renders a scene or decides what a
//! record means beyond what the files state; a field whose role is not known is kept as stored. Every length, count
//! and offset is checked against the bytes it indexes, and a malformed file is refused with an [`Error`] that names the
//! offset, never a panic.
//!
//! - Containers: [`pak`] reads PAK v4 archives (big endian, named entries in sections, stored or [`cstr`] blocks), and
//!   [`cstr`] the CSTR framing of one compressed entry.
//! - Level geometry: a level's meshes are quartets of `.scp`, `.mlp`, `.mmp` and `.mgp` files. [`scene::decode`] reads
//!   a quartet into models (positions, [`attributes`], submeshes, processed [`skin`] records) and placements;
//!   [`scp_tables`] reads the SCP's node and world-matrix tables and its blob sections of spot chunks, [`scp_splines`],
//!   [`scp_cameras`], [`scp_lights`], [`scp_locators`] and [`scp_occluders`].
//! - Materials: [`material`] reads the MBP/MDP material banks, [`pixel_program`] classifies a compiled fragment program
//!   with [`rsx_reader`], [`texture`] indexes the packaged texture headers and pixel payloads, and [`library`] joins
//!   banks, programs and textures.
//! - The disc: [`disc`] lists where its levels, layers, database and shared archives are.
//! - Tables: [`database`] reads `MAIN.DB` (SQLite 3) whole through `quantydb-sqlite`; [`session`], [`surfaces`],
//!   [`manifest`] and [`names`] read the level, session, surface, drop-spot and rail rows of it.
//! - Scripts: [`level_params`] lists the constant-argument calls of a level's Lua 5.1 parameter script.
//! - Animation: [`reference_rig`] reads the Morpheme reference rigs of an animation container.
//!
//! Matrices are [`Affine`] values in the source frame (left-handed, Y up); converting them is the caller's.
//!
//! # Example
//!
//! Open an archive from your own disc and decode every level mesh in it (this compiles but is not run, as it needs
//! the game's files; `examples/list_quartets.rs` is the same code as a program):
//!
//! ```no_run
//! use rme_formats::{pak::Pak, scene};
//!
//! let pak = Pak::open("PS3_GAME/USRDIR/LEVELS/CHARLIE.PAK")?;
//! for (stem, entries) in scene::quartets(&pak)? {
//!     let scene = scene::decode_quartet(&pak, entries)?;
//!     println!(
//!         "{}: {} models, {} placements",
//!         String::from_utf8_lossy(&stem),
//!         scene.models.len(),
//!         scene.placements.len()
//!     );
//! }
//! # Ok::<(), rme_formats::Error>(())
//! ```
//!
//! Bytes that are not a readable file are refused, with the offset and what was expected:
//!
//! ```
//! use rme_formats::pak::Pak;
//!
//! // the magic, the endian tag, then version 5
//! let header = [b'P', b'A', b'K', 0, 0xfe, 0xeb, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0];
//! let error = Pak::from_bytes(header.to_vec()).unwrap_err();
//! assert_eq!(error.to_string(), "at 0x6: unsupported PAK version");
//! ```
#![deny(clippy::disallowed_methods, clippy::disallowed_types, missing_docs)]
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation
    )
)]

pub mod attributes;
pub mod cstr;
pub mod database;
pub mod disc;
pub mod level_params;
pub mod library;
pub mod manifest;
pub mod material;
pub mod names;
pub mod pak;
pub mod pixel_program;
pub mod reference_rig;
pub mod scene;
pub mod scp_cameras;
pub mod scp_lights;
pub mod scp_locators;
pub mod scp_occluders;
pub mod scp_splines;
pub mod scp_tables;
pub mod session;
pub mod skin;
pub mod surfaces;
pub mod texture;

/// Why a read was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading the file from disk failed (only [`pak::Pak::open`] touches the file system).
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// A named field lies outside the bytes being read; the message gives its offset.
    #[error(transparent)]
    Field(#[from] primase_foundations::bytes::FieldError),
    /// The data is not what the format allows, at `offset` in the file or decoded entry being read.
    #[error("at {offset:#x}: {detail}")]
    Invalid {
        /// Where the bad bytes are, in the file or decoded entry being read.
        offset: usize,
        /// What was expected there.
        detail: String,
    },
    /// A texture header or pixel copy was refused; the inner error keeps its own offset.
    #[error(transparent)]
    Texture(Box<crate::texture::Error>),
}

/// A read that is either done or refused with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(offset: usize, detail: impl ToString) -> Error {
    Error::Invalid {
        offset,
        detail: detail.to_string(),
    }
}

/// An affine 3x4 transform, column by column: the images of local +X, +Y, +Z, then the translation;
/// `world = M * local`. Entries are the stored f32 values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// The three columns of the linear part, then the translation, each as x, y, z.
    pub cols: [[f32; 3]; 4],
}

impl Affine {
    /// The transform that changes nothing: identity linear part, zero translation.
    pub const IDENTITY: Affine = Affine {
        cols: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [0., 0., 0.]],
    };

    /// `self` after `inner` (`world = self (inner local)`), computed in f64 from the stored values with each entry
    /// rounded once to f32.
    pub fn compose(&self, inner: &Affine) -> Affine {
        let [[a0, a1, a2], [b0, b1, b2], [c0, c1, c2], [t0, t1, t2]] = self.cols.map(|c| c.map(f64::from));
        let linear = |[v0, v1, v2]: [f32; 3]| -> [f64; 3] {
            let (v0, v1, v2) = (f64::from(v0), f64::from(v1), f64::from(v2));
            [
                a0 * v0 + b0 * v1 + c0 * v2,
                a1 * v0 + b1 * v1 + c1 * v2,
                a2 * v0 + b2 * v1 + c2 * v2,
            ]
        };
        let [x, y, z, [u0, u1, u2]] = inner.cols.map(linear);
        Affine {
            cols: [
                x.map(round_f32),
                y.map(round_f32),
                z.map(round_f32),
                [round_f32(u0 + t0), round_f32(u1 + t1), round_f32(u2 + t2)],
            ],
        }
    }
}

/// The nearest f32 to `value`: the single rounding that [`Affine::compose`] promises.
#[allow(
    clippy::cast_possible_truncation,
    reason = "rounds a computed f64 to f32 by design; it is not a stored length or offset"
)]
fn round_f32(value: f64) -> f32 {
    value as f32
}

impl From<crate::texture::Error> for Error {
    fn from(error: crate::texture::Error) -> Self {
        Self::Texture(Box::new(error))
    }
}

/// `base + delta`, refused when it overflows: stored offsets and sizes are summed through this, never with `+`.
pub(crate) fn offset(base: usize, delta: usize) -> Result<usize> {
    base.checked_add(delta)
        .ok_or_else(|| invalid(base, "offset overflows"))
}

/// `base + index * stride + field`: the offset of `field` in record `index` of a table at `base`, refused when any
/// step overflows.
pub(crate) fn element(base: usize, index: usize, stride: usize, field: usize) -> Result<usize> {
    index
        .checked_mul(stride)
        .and_then(|n| n.checked_add(base))
        .and_then(|n| n.checked_add(field))
        .ok_or_else(|| invalid(base, "record offset overflows"))
}
