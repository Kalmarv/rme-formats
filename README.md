# rme-formats

[![crates.io: rme-formats](https://img.shields.io/crates/v/rme-formats?label=rme-formats)](https://crates.io/crates/rme-formats) [![docs.rs](https://img.shields.io/docsrs/rme-formats)](https://docs.rs/rme-formats) [![CI](https://img.shields.io/github/actions/workflow/status/Kalmarv/rme-formats/ci.yml?branch=main&label=CI)](https://github.com/Kalmarv/rme-formats/actions/workflows/ci.yml)

A Rust library that reads the game files of Tony Hawk: Shred (PlayStation 3, 2010), which runs on Robomodo's RME engine. It is used by [primase](https://github.com/Kalmarv/primase), which converts the game's assets.

## Features

- Reads PAK v4 archives, including entries compressed as CSTR blocks of raw DEFLATE
- Decodes level mesh quartets (`.scp`, `.mlp`, `.mmp`, `.mgp`) into models, positions, indices, submeshes and placements
- Decodes packed vertex attributes: RSX CMP normals, vertex colours and half-float UV sets
- Reads processed skin meshes with their bone palettes, weights and source skeleton
- Reads the SCP node and world-matrix tables and its spot chunks, splines, spot cameras, lights, locators and occluders
- Reads the MBP/MDP material banks and joins them to their fragment programs and texture headers
- Indexes texture headers (`.thp`, a `CellGcmTexture` descriptor) and the streamed, prefetch and cube-tail pixel copies (`.txp`)
- Classifies fragment programs that only sample one texture and scale it by uniforms
- Reads the level, session, goal, task, surface and drop-spot tables of `MAIN.DB` (SQLite 3)
- Sorts a level's base and layer archives by difficulty, session type and map id, and finds them in an extracted disc folder
- Reads the literal-argument calls of a level's Lua 5.1 parameter script
- Reads Morpheme reference rigs from animation containers and builds their reference matrices
- Keeps fields of unknown meaning as stored and lists the byte spans it does not decode
- Refuses malformed input with an error that names the offset, and contains no `unsafe` code

## Getting Started

### Requirements

- Rust 1.99 or later (edition 2024). The repository pins 1.99.0 in `rust-toolchain.toml`.
- Your own copy of the game: the `PS3_GAME/USRDIR` folder extracted unchanged from the disc. The repository ships no game files.

### Installation

The last release, 0.3.0, is on crates.io:

```sh
cargo add rme-formats
```

The `main` branch has changes that are not released yet: the `disc` module, and `MAIN.DB` is read through `primase_foundations::sqlite` instead of the old `database` module. To use them, depend on the repository:

```toml
[dependencies]
rme-formats = { git = "https://github.com/Kalmarv/rme-formats" }
```

### Usage

This program opens an archive from your disc and decodes every level mesh in it. It is `examples/list_quartets.rs`.

```rust
use rme_formats::{pak::Pak, scene};

fn main() -> Result<(), rme_formats::Error> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: list_quartets <archive.pak>");
    let pak = Pak::open(path)?;
    for (stem, entries) in scene::quartets(&pak)? {
        let scene = scene::decode_quartet(&pak, entries)?;
        println!(
            "{}: {} models, {} placements, {} unread spans",
            String::from_utf8_lossy(&stem),
            scene.models.len(),
            scene.placements.len(),
            scene.unread.len()
        );
    }
    Ok(())
}
```

```sh
cargo run --example list_quartets -- path/to/PS3_GAME/USRDIR/LEVELS/CHARLIE.PAK
```

Other entry points:

- `disc::Disc` lists the base levels, layer archives, database and material banks of a disc folder.
- `library::MaterialLibrary::new` reads the material banks of `MATERIALS.PAK` with the programs and textures of the archives you pass.
- `texture::TextureCatalog::pixel_data` returns a texture's descriptor, first stored mip and stored bytes; decoding the pixels is left to the caller.
- `session::SessionGraph::from_main_db` reads one level's session, goal and task rows.
- `level_params::read_level_params` reads a level script from the scripts archive. The game stores that archive encrypted in `SCRIPTS.SDAT`, so it must be decrypted first, for example with `rpcs3 --headless --decrypt`.
- `reference_rig::read_archive` reads the reference rigs of an animation container.

Matrices are returned in the game's own frame (left-handed, Y up). [docs/scope.md](docs/scope.md) lists what the readers check and what they leave to the caller.

`cargo test --workspace` runs the tests on hand-built fixtures and needs no game files. The tests that read a real disc skip unless the `GAME_DATA` environment variable points at a configuration file; [docs/real-data-tests.md](docs/real-data-tests.md) describes it. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full check list and how to report a file the library cannot read.

Licensed under GPL-3.0-or-later; see [LICENSE](LICENSE).
