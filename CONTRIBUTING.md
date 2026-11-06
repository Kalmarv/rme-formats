# Contributing

## Build and test

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on first use.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

The library code denies `clippy::indexing_slicing`, `clippy::arithmetic_side_effects` and
`clippy::cast_possible_truncation`: read bytes through `View` or `scene::Bytes`, sum stored offsets with
`crate::offset` or `crate::element`, and convert sizes with `try_from`. The test code is exempt.

`cargo test` needs no game files. The tests that read a real disc skip, and print the key they need, unless the
environment variable `GAME_DATA` names a TOML file that points at your own copy of the game; the README's "Tests"
section and [tests/inputs/README.md](tests/inputs/README.md) say how. A change to a reader should come with a test on a
hand-built fixture (see `tests/archive.rs` for the style: the expected bytes are written out, not made by the code under
test). If you change which real-data inputs a test reads, declare them in `tests/inputs/mod.rs`; the
`real_data_inputs` test fails until `tests/inputs/README.md` shows the same table.

## Releasing

1. Raise the version (a patch, 0.x.Y, for fixes and data or manifest additions; a minor, 0.X.0, for new API or any
   breaking change before 1.0; then `cargo check`, so `Cargo.lock` follows) and put the change's CHANGELOG lines under a
   `## <version> (<date>)` heading, in one commit.
2. Push it to main. Once every check has passed, release-plz publishes each crate whose version crates.io does not have
   yet (trusted publishing: no token is stored anywhere), tags the commit `v<version>` and creates the GitHub Release
   with that CHANGELOG section as its notes.

A commit that raises no version only runs CI. Never release by hand.

A new crate's first version can't come from trusted publishing: the token holder publishes it by hand, then adds its
trusted-publishing config (workflow `ci.yml`).

## Reporting a file the library cannot read

Open an issue with the "Unreadable file" template. Please include:

- the full error message (it names the offset and what was expected) and the function you called;
- the name of the file inside its archive (for example `levels\charlie\charlie.scp`), the archive's file name, the size in
  bytes and the SHA-256 of the file;
- the version or commit of this library, and the disc region or version if you know it;
- if you can, the 16 bytes around the reported offset as hex.

Please do not attach game files: not the disc image, an archive, an extracted file, or textures and models made from
them. They are copyrighted, and the report is useful without them. If a maintainer needs more bytes they will say which.
