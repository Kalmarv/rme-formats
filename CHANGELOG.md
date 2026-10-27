# Changelog

## 0.2.0 (2026-10-27)

- Depends on `dx` at a revision where `rsx-reader` names the fragment operand bit `interpolant_flag` (it was
  `perspective_input`); the pixel-program classifier reads it as before.
- Depends on `dx` 0.2.0 (`rsx-reader` 0.2): `Texture::gcm` is its `CellGcmTexture`, so this release is breaking too.

## 0.1.0 (2026-10-27)

- First release: Tony Hawk's SHRED (PS3) PAK archives, level meshes, materials, textures, MAIN.DB, reference rigs and level scripts.
