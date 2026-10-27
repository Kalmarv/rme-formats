# Real-data inputs

The files this crate's real-data tests read, as `tests/inputs/mod.rs` declares them. None of them is shipped: each is
made from your own copy of the game. A test whose input is not configured skips and prints one line naming the key.

Point the environment variable `GAME_DATA` at a TOML file that holds the keys below (the layout of that file is
described in [primase-testdata](https://github.com/Kalmarv/primase-testdata#the-configuration-file)); `GAME_DATA_DEBUG`
sets the debug switches a test declares, if any. A `gate` of "yes" means the input is needed for the full test run:
with `[run] require = 'gate'` in the TOML file an unset gate input fails instead of skipping.

<!-- real-data inputs: made by primase_testdata::doc::tables from tests/inputs/mod.rs -->

#### Configuration keys

| key | gate | about | source |
| --- | --- | --- | --- |
| `installs.shred` | yes | Tony Hawk: SHRED's (PS3, 2010) disc folder: `PS3_GAME/USRDIR` with its PAK archives | the user's own disc, extracted unchanged |
| `scratch.root` | - | where real-data tests write their outputs (the OS temporary folder when unset) | - |

#### Oracle sets (`<oracles.root>/<set>/<game>/`)

| set | about | source |
| --- | --- | --- |
| `oracles.corpus` | reference files derived from the user's own install by an independent reader (here, a dump of MAIN.DB) | made from the user's own disc |
| `oracles.rpcs3` | RPCS3's RSX fragment-program disassembly, produced by a batch driver around RPCS3's own decoder | independent: RPCS3's Cg fragment-program decoder (TaskFP) at upstream commit 979a52415003fd7a74ddd216e1a5782b3731281f, built unchanged as a command-line tool |

#### Declared files

| key | status | gate | about | source |
| --- | --- | --- | --- | --- |
| `installs.shred/PS3_GAME/USRDIR/database/main.db` | live | yes | Tony Hawk: SHRED's SQLite 3 database of levels, sessions, goals and drop spots (142 tables); tests/main_db.rs | the user's own disc, extracted unchanged |
| `oracles.corpus.shred/main-db.tsv` | live | yes | TSV of every table of SHRED's MAIN.DB (sqlite_master first): `@table` and its column names, then one line per row, its rowid and typed values (`n`, `i:`, `r:` big-endian double, `t:` UTF-8 and `b:` as hex); tests/main_db.rs | independent: Python's standard-library sqlite3 (SQLite's own C library) reading the database read-only, tests/oracles/sqlite_dump.py |
| `oracles.rpcs3.shred/fragment` | live | yes | per-microcode SHA256 fragment disassembly plus occurrence index: 3267 distinct microcodes covering all 181851 packaged fragment programs; tests/fragment_disc.rs | independent: RPCS3's Cg fragment-program decoder (TaskFP) at upstream commit 979a52415003fd7a74ddd216e1a5782b3731281f, built unchanged as a command-line tool and run over every distinct microcode; represented operands and control boundaries compared, omitted modifiers and sampling operands checked with hand-worked fixtures; disagreements are settled with fixtures rather than deferring to this tool |

<!-- end of real-data inputs -->
