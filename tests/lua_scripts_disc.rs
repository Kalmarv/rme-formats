//! Every Lua script of Tony Hawk: SHRED's decrypted scripts archive (1,278 chunks) through `lua51-bytecode`'s whole
//! gate: header, verifier, depth and trailing bytes. None is refused, all are the stock little-endian header with
//! the debug information stripped, and every function ends in `RETURN` with `B == 1`.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod inputs;

use inputs::{Game, OracleSet, oracle};
use lua51_bytecode::{Chunk, Endian, Op, Proto};
use rme_formats::pak::Pak;
use std::collections::BTreeMap;

/// What the level-script reader treats as control flow: any nested function, or one of these opcodes in the main
/// function (`level_params::read_level_params` reads no such script).
fn branching(main: &Proto) -> bool {
    !main.protos.is_empty()
        || main.code.iter().any(|i| {
            matches!(
                i.op,
                Op::Jmp
                    | Op::Eq
                    | Op::Lt
                    | Op::Le
                    | Op::Test
                    | Op::TestSet
                    | Op::ForLoop
                    | Op::ForPrep
                    | Op::TForLoop
                    | Op::Closure
            )
        })
}

#[derive(Default, Debug)]
struct Functions {
    total: usize,
    nested: usize,
    stripped: usize,
    /// How many functions end in `RETURN` with each `B`.
    last_return_b: BTreeMap<u16, usize>,
}

fn walk(p: &Proto, out: &mut Functions, nested: bool) {
    out.total += 1;
    out.nested += usize::from(nested);
    out.stripped += usize::from(
        p.source.is_none() && p.lines.is_empty() && p.locals.is_empty() && p.upvalue_names.is_empty(),
    );
    let last = p.code.last().unwrap();
    assert_eq!(last.op, Op::Return);
    *out.last_return_b.entry(last.b).or_default() += 1;
    for n in &p.protos {
        walk(n, out, true);
    }
}

#[test]
fn every_script_passes_the_whole_gate() {
    let Some(path) = oracle(OracleSet::Corpus, Game::Shred, "scripts.pak") else {
        return;
    };
    let pak = Pak::open(path).unwrap();
    let mut refusals: BTreeMap<String, usize> = BTreeMap::new();
    let mut headers = BTreeMap::new();
    let mut functions = Functions::default();
    let (mut scripts, mut bytes, mut with_nested, mut branching_scripts) = (0, 0, 0, 0);
    for (i, e) in pak.entries().iter().enumerate() {
        if !(e.name.starts_with(b"script\\") && e.name.ends_with(b".lua")) {
            continue;
        }
        scripts += 1;
        let payload = pak.payload(i).unwrap();
        bytes += payload.len();
        match Chunk::parse(&payload) {
            Err(err) => *refusals.entry(err.detail).or_default() += 1,
            Ok(chunk) => {
                let h = chunk.header;
                let key = (h.endian == Endian::Little, h.int_size, h.size_t_size);
                *headers.entry(key).or_insert(0usize) += 1;
                with_nested += usize::from(!chunk.main.protos.is_empty());
                branching_scripts += usize::from(branching(&chunk.main));
                walk(&chunk.main, &mut functions, false);
            }
        }
    }
    eprintln!(
        "SHRED scripts: {scripts} ({bytes} bytes) refusals {refusals:?}, headers {headers:?}, {with_nested} with nested functions, {branching_scripts} branching, {functions:?}"
    );
    assert_eq!((scripts, bytes), (1278, 3_496_540));
    assert!(refusals.is_empty(), "{refusals:?}");
    assert_eq!(headers, BTreeMap::from([((true, 4, 4), 1278)]));
    assert_eq!(with_nested, 5);
    assert_eq!(branching_scripts, 583);
    assert_eq!(functions.total, 1278 + 45);
    assert_eq!(functions.nested, 45);
    assert_eq!(functions.stripped, functions.total);
    assert_eq!(functions.last_return_b, BTreeMap::from([(1, functions.total)]));
}
