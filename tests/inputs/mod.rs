//! The real-data inputs this crate's tests read, each with what it is, where it comes from and whether the
//! full test run requires it (`gate: true`; see `primase_testdata::inputs!`). README.md in this folder lists
//! them. Integration tests reach this module with `mod inputs;`, and tests/real_data_inputs.rs checks that every
//! query the crate's sources write is declared here. Declare an input here when a test first asks for it.

primase_testdata::inputs! {
    games {
        Shred = "shred",
    }
    installs {
        Shred {
            about: "Tony Hawk: SHRED's (PS3, 2010) disc folder: `PS3_GAME/USRDIR` with its PAK archives",
            source: "the user's own disc, extracted unchanged",
            gate: true,
        },
    }
    keys {}
    tools {}
    oracle_sets {
        Corpus = "corpus" {
            about: "reference files derived from the user's own install by an independent reader (here, a dump of MAIN.DB)",
            source: "made from the user's own disc",
        },
        Rpcs3 = "rpcs3" {
            about: "RPCS3's RSX fragment-program disassembly, produced by a batch driver around RPCS3's own decoder",
            source: "independent: RPCS3's Cg fragment-program decoder (TaskFP) at upstream commit 979a52415003fd7a74ddd216e1a5782b3731281f, built unchanged as a command-line tool",
        },
    }
    declared {
        Query::Install(Game::Shred, Some("PS3_GAME/USRDIR/database/main.db")) => {
            status: Live,
            gate: true,
            about: r#"Tony Hawk: SHRED's SQLite 3 database of levels, sessions, goals and drop spots (142 tables); tests/main_db.rs"#,
            source: r#"the user's own disc, extracted unchanged"#,
        },
        Query::Oracle(OracleSet::Corpus, Game::Shred, "main-db.tsv") => {
            status: Live,
            gate: true,
            about: r#"TSV of every table of SHRED's MAIN.DB (sqlite_master first): `@table` and its column names, then one line per row, its rowid and typed values (`n`, `i:`, `r:` big-endian double, `t:` UTF-8 and `b:` as hex); tests/main_db.rs"#,
            source: r#"independent: Python's standard-library sqlite3 (SQLite's own C library) reading the database read-only, tests/oracles/sqlite_dump.py"#,
        },
        Query::Oracle(OracleSet::Rpcs3, Game::Shred, "fragment") => {
            status: Live,
            gate: true,
            about: "per-microcode SHA256 fragment disassembly plus occurrence index: 3267 distinct microcodes covering all 181851 packaged fragment programs; tests/fragment_disc.rs",
            source: "independent: RPCS3's Cg fragment-program decoder (TaskFP) at upstream commit 979a52415003fd7a74ddd216e1a5782b3731281f, built unchanged as a command-line tool and run over every distinct microcode; represented operands and control boundaries compared, omitted modifiers and sampling operands checked with hand-worked fixtures; disagreements are settled with fixtures rather than deferring to this tool",
        },
    }
    debug {}
}
