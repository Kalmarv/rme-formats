# Real-data tests

`cargo test --workspace` runs the synthetic tests: fixtures built byte by byte, plus one SQLite file written by
Python's `sqlite3` (`tests/oracles/session_fixture.py`). They need no game files.

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

[`tests/inputs/README.md`](../tests/inputs/README.md) lists every input a test reads, what it is and how it is made (a
table generated from `tests/inputs/mod.rs`). The layout of the file and its other keys are described in
[primase-testdata](https://github.com/Kalmarv/primase-testdata#the-configuration-file), which loads it.

Two oracle sets are used:

- `corpus`: a dump of `MAIN.DB` made by `tests/oracles/sqlite_dump.py`, and the decrypted scripts archive
  (`scripts.pak`, decrypted from `SCRIPTS.SDAT` with `rpcs3 --headless --decrypt`).
- `rpcs3`: disassemblies of every distinct fragment and vertex program, made with RPCS3's own decoders.

Tests whose oracle is not configured skip.
