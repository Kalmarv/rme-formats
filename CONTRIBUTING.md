# Contributing

## Build and test

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on first use.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

`cargo test` needs no game files. The tests that read a real disc skip, and print the key they need, unless the
environment variable `GAME_DATA` names a TOML file that points at your own copy of the game; the README's "Tests"
section and [tests/inputs/README.md](tests/inputs/README.md) say how. A change to a reader should come with a test on a
hand-built fixture (see `tests/archive.rs` for the style: the expected bytes are written out, not made by the code under
test). If you change which real-data inputs a test reads, declare them in `tests/inputs/mod.rs`; the
`real_data_inputs` test fails until `tests/inputs/README.md` shows the same table.

## Reporting a file the library cannot read

Open an issue with the "Unreadable file" template. Please include:

- the full error message (it names the offset and what was expected) and the function you called;
- the name of the file inside its archive (for example `levels\charlie\charlie.scp`), the archive's file name, the size in
  bytes and the SHA-256 of the file;
- the version or commit of this library, and the disc region or version if you know it;
- if you can, the 16 bytes around the reported offset as hex.

Please do not attach game files: not the disc image, an archive, an extracted file, or textures and models made from
them. They are copyrighted, and the report is useful without them. If a maintainer needs more bytes they will say which.
