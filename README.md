# rme-formats

A Rust library that reads the game data of Tony Hawk's SHRED for PS3: archives, level meshes, materials, textures, the game's database and its level scripts.

Tony Hawk's SHRED (2010) runs on Robomodo's RME engine. The library is written against the PS3 disc (32-bit
big-endian pointers, the `PS3_GAME/USRDIR` folder) and turns its files into plain Rust records: bytes in, structs out,
nothing rendered, nothing written back.

| module | reads |
|---|---|
| `pak`, `cstr` | PAK v4 archives (big endian; names kept as bytes; stored entries or CSTR v1 blocks of raw DEFLATE) |
| `scene` | a level mesh quartet (`.scp`, `.mlp`, `.mmp`, `.mgp`): models, positions, indices, submeshes, placements |
| `attributes`, `skin` | packed vertex attributes (RSX CMP normals, halves, colours, UVs) and processed skin records |
| `scp_tables`, `scp_splines`, `scp_cameras`, `scp_lights`, `scp_locators`, `scp_occluders` | an SCP's nodes, world matrices and blob sections: spot chunks, splines, cameras, lights, locators, occluders |
| `material`, `pixel_program`, `texture`, `library` | MBP/MDP material banks, fragment programs (uniform names and a microcode classification), 84-byte texture headers with their packaged pixel copies, and the three joined |
| `database`, `session`, `surfaces`, `manifest`, `names` | `MAIN.DB` (SQLite 3) tables, the level/session/goal/task rows, collision surface names, which archives hold a level, spline names |
| `level_params` | the constant-argument calls of a level's Lua 5.1 parameter script |
| `reference_rig` | the Morpheme reference rigs of an animation container |

## Example

Open an archive from your own disc and decode every level mesh in it. The crate documentation compiles the same code as a doc test (it is not run, since it needs game files), and `examples/list_quartets.rs` is a runnable version that takes the archive path: `cargo run --example list_quartets -- <archive.pak>`.

```rust
use rme_formats::{pak::Pak, scene};

fn main() -> Result<(), rme_formats::Error> {
    let pak = Pak::open("PS3_GAME/USRDIR/LEVELS/CHARLIE.PAK")?;
    for (stem, entries) in scene::quartets(&pak)? {
        let scene = scene::decode_quartet(&pak, entries)?;
        println!(
            "{}: {} models, {} placements",
            String::from_utf8_lossy(&stem),
            scene.models.len(),
            scene.placements.len()
        );
    }
    Ok(())
}
```

## What it guarantees

- Lengths, counts and offsets are checked against the bytes they index, and the readers are written to return an error
  instead of panicking on a malformed file. A refusal names the offset (and, for a quartet, the file within it) and what
  was expected. The tests check this on hand-damaged fixtures; the crate has not been fuzzed.
- Fields whose role is unproven are kept as stored (opaque words, unknown tables listed with their byte spans), not
  guessed. Names keep their original bytes, case and separators.
- Decoded values are not repaired. A non-finite matrix element, a position outside its stored bounds, an index out of
  range or a non-unit RSX CMP normal is refused; zero normals and the NaN normals of some skin vertices are kept as
  stored. A mesh variant the library does not support is marked refused while the other models of the file decode.
- Matrices are `Affine` (3x4, column by column) in the game's native frame: left-handed, Y up. Converting frames is the
  caller's.
- Only `Pak::open` touches the file system; everything else takes bytes. `unsafe` code is denied.

## What it does not do

- It does not decode pixels: `texture` returns the `CellGcmTexture` descriptor (read by
  [`rsx-reader`](https://github.com/Kalmarv/dx)), the first stored mip and the stored bytes.
- It does not decrypt the game's scripts archive, choose which archives make a level, or pick a session; it lists and
  the caller chooses.
- It does not write any format, render, or decode animation clips (only the reference rigs).
- It reads the PS3 disc layout only. No other platform or Robomodo title has been tried.
- Its API is not settled: as a 0.x crate, a minor release (0.2) may change it.

## Tests

`cargo test --workspace` runs the synthetic tests: hand-laid fixtures built byte by byte, plus two SQLite files written
by Python's `sqlite3` (`tests/oracles/`). They need no game files. The toolchain is pinned in `rust-toolchain.toml`.

The tests named `*_disc`, and `main_db`, read your own copy of the game and print `SKIP` with the key they need when it
is not configured. To run them, write a TOML file and point the environment variable `GAME_DATA` at it:

```toml
[installs]
shred = 'D:/games/shred'          # the folder that holds PS3_GAME/USRDIR, extracted unchanged from your disc

[oracles]
root = 'D:/oracles'               # <root>/<set>/shred/...: reference files made by independent readers

[run]
gate = 'smoke'                    # or 'full': how much the heavy tests cover (smoke by default)
```

[`tests/inputs/README.md`](tests/inputs/README.md) lists every input a test reads, what it is and how it is made (a
table generated from `tests/inputs/mod.rs`). The layout of the file and its other keys are described in
[primase-testdata](https://github.com/Kalmarv/primase-testdata#the-configuration-file), which loads it. Two oracle
sets are used: a dump of `MAIN.DB` made by `tests/oracles/sqlite_dump.py`, and a disassembly of every distinct fragment
program made with RPCS3's Cg decoder; without them the tests that compare against them skip.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md), which also says how to report a file the library cannot read.

## License

GPL-3.0-or-later; see [LICENSE](LICENSE).

Used by [primase](https://github.com/Kalmarv/primase), a game-asset conversion library.
