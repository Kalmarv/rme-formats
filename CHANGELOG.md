# Changelog

## 0.5.0 (2026-11-06)

- **Breaking:** depends on `dx` 0.5.0, whose `rsx-reader` type `gcm::CellGcmTexture` is the type of
  `TextureHeader::gcm` (the same source as 0.4.0's).

## 0.4.0 (2026-11-06)

- `disc`: the disc's layout (`PS3_GAME/USRDIR`, `MAIN.DB`, `MATERIALS.PAK`, the shared `RME.PAK` and `VFX.PAK`) and its
  listings: the base levels, read from the `levels` table and the archives present, and the layer archives.
- `manifest::levels`: the `levels` table's rows alone.
- tests/disc_levels.rs: the disc's ten base levels and 209 layer archives.
- **Breaking:** the `database` module moved to primase-foundations 0.3.1 as `sqlite` (feature `sqlite`), unchanged;
  this crate reads `MAIN.DB` through it and no longer depends on `quantydb-sqlite`. Its synthetic tests and their
  fixture moved with it; the tests that read SHRED's own `MAIN.DB` stay here.

## 0.3.0 (2026-10-29)

- **Breaking:** depends on `dx` 0.4.0, whose `rsx-reader` type `gcm::CellGcmTexture` is the type of
  `TextureHeader::gcm`; and on primase-foundations 0.3.0, lua51-bytecode 0.2.0 and morpheme-formats 0.1.1.
- tests/vertex_disc.rs: every vertex program of SHRED's disc decodes and agrees with RPCS3's disassembly.

## 0.2.0 (2026-10-27)

- Depends on `dx` at a revision where `rsx-reader` names the fragment operand bit `interpolant_flag` (it was
  `perspective_input`); the pixel-program classifier reads it as before.
- Depends on `dx` 0.2.0 (`rsx-reader` 0.2): `Texture::gcm` is its `CellGcmTexture`, so this release is breaking too.

## 0.1.0 (2026-10-27)

- First release: Tony Hawk's SHRED (PS3) PAK archives, level meshes, materials, textures, MAIN.DB, reference rigs and level scripts.
