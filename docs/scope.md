# Scope

What the readers check, and what they leave to the caller.

## What the readers check

- Lengths, counts and offsets are checked against the bytes they index. The readers return an error instead of
  panicking on a malformed file, and the error names the offset (and, for a quartet, the file within it) and what was
  expected. The tests check this by replacing every aligned word of hand-built fixtures with boundary values; the crate
  has not been fuzzed.
- Fields whose role is unproven are kept as stored (opaque words, unknown tables listed with their byte spans), not
  guessed. Names keep their original bytes, case and separators.
- Decoded values are not repaired. A non-finite matrix element, a position outside its stored bounds, an index out of
  range or a non-unit RSX CMP normal is refused; zero normals and the NaN normals of some skin vertices are kept as
  stored. A mesh variant the library does not support is marked refused while the other models of the file decode.
- Matrices are `Affine` (3x4, column by column) in the game's native frame: left-handed, Y up. Converting frames is the
  caller's.
- `Pak::open` and the `disc` listings are the only parts that touch the file system; everything else takes bytes.
  `unsafe` code is denied.

## What is left to the caller

- Pixels are not decoded: `texture` returns the `CellGcmTexture` descriptor (read by
  [`rsx-reader`](https://github.com/Kalmarv/dx)), the first stored mip and the stored bytes.
- The game's scripts archive (`SCRIPTS.SDAT`) is not decrypted. The library does not choose which archives make a
  level or pick a session; it lists them and the caller chooses.
- No format is written, nothing is rendered, and animation clips are not decoded (only the reference rigs).
- Only the PS3 disc layout is read. No other platform or Robomodo title has been tried.
- The API is not settled: as a 0.x crate, a minor release may change it.
