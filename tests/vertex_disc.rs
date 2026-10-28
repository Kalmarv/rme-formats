//! The RSX vertex microcode of every packaged vertex program, decoded and checked against the program's own header: the
//! instruction count, the registers it uses, the attributes it reads (`attribute_input_mask`) and the outputs it writes
//! (`attribute_output_mask`) are what the decoded instructions say. A distinct program is decoded once; a repeated one is
//! byte-compared to it. Each distinct microcode is also compared with RPCS3's own vertex disassembler (an independent
//! implementation of the same format, run over every distinct microcode): the operations it prints, their write masks, the
//! sources with their registers, swizzles, negation and absolute value, the saturate and condition fields, and where each
//! result goes.
mod inputs;

use inputs::{Depth, Game, OracleSet, Query, depth, expect_found, install, oracle, seed};
use rme_formats::pak::Pak;
use rsx_reader::cgbinary::{Stage, parse_prefix};
use rsx_reader::vertex::{self, Destination, ScalarOpcode, SourceKind, VectorOpcode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The operations RPCS3 prints for a slot, in its order (the scalar operation, then the vector one), each with the sources
/// it prints: the scalar operation reads source 2; a vector operation reads the sources its opcode uses.
fn printed(ins: &vertex::Instruction) -> Vec<(bool, &'static str, Vec<usize>)> {
    let mut out = Vec::new();
    if ins.scalar != ScalarOpcode::NOP {
        out.push((true, ins.scalar.mnemonic(), vec![2]));
    }
    if ins.vector != VectorOpcode::NOP {
        let sources = match ins.vector.mnemonic() {
            "MOV" | "FRC" | "FLR" | "SFL" | "STR" | "SSG" => vec![0],
            "MUL" | "DP3" | "DPH" | "DP4" | "DST" | "MIN" | "MAX" | "SLT" | "SGE" | "SEQ" | "SGT" | "SLE"
            | "SNE" => {
                vec![0, 1]
            }
            "ADD" => vec![0, 2],
            "MAD" => vec![0, 1, 2],
            other => {
                panic!("{other}: RPCS3 prints address and texture operands this comparison does not model")
            }
        };
        out.push((false, ins.vector.mnemonic(), sources));
    }
    out
}

/// RPCS3's spelling of a swizzle: a replicated lane is one letter, the identity none.
fn lanes(s: [u8; 4]) -> String {
    let letters: String = s.iter().map(|&n| ['x', 'y', 'z', 'w'][usize::from(n)]).collect();
    if letters == "xyzw" {
        String::new()
    } else if s.iter().all(|&n| n == s[0]) {
        letters[..1].to_owned()
    } else {
        letters
    }
}

/// Compares a decoded program with RPCS3's printed operations; returns how many operations were compared and how many
/// destinations RPCS3 prints as two (its "double destination", which it prints whenever a slot's vector temporary field is
/// set, whichever operation owns the output).
fn compare(program: &vertex::Program, reference: &Value, label: &str) -> (usize, usize) {
    let operations = reference["operations"].as_array().expect("operations");
    let (mut at, mut doubles) = (0, 0);
    for ins in &program.instructions {
        for (scalar, name, sources) in printed(ins) {
            let op = operations
                .get(at)
                .unwrap_or_else(|| panic!("{label}: RPCS3 printed {at} operations, the program has more"));
            at += 1;
            let here = format!("{label}: slot at {} {name}", ins.offset);
            assert_eq!(op["op"], name, "{here}");
            assert_eq!(op["sat"], ins.saturate, "{here}: saturate");
            assert_eq!(
                op["condition_update"], ins.condition.update,
                "{here}: condition update"
            );
            let expected = if ins.condition.test && ins.condition.code != 7 {
                let test =
                    ["ERROR", "LT", "EQ", "LE", "GT", "NE", "GE", "ERROR"][usize::from(ins.condition.code)];
                let l = lanes(ins.condition.swizzle);
                Some(if l.is_empty() {
                    test.to_owned()
                } else {
                    format!("{test}.{l}")
                })
            } else {
                None
            };
            assert_eq!(
                op["condition"],
                expected.map_or(Value::Null, Value::from),
                "{here}: condition"
            );
            let (mask, destination) = if scalar {
                (ins.scalar_mask, ins.scalar_destination)
            } else {
                (ins.vector_mask, ins.vector_destination)
            };
            let printed_destination = &op["destination"];
            let extra = &op["extra_temp"];
            assert_eq!(printed_destination["mask"], mask, "{here}: write mask");
            match destination {
                Destination::Temporary(n) => {
                    // RPCS3 prints a temporary as the destination, or after an output as the "double destination"
                    let in_extra = extra["temp"] == n;
                    assert!(
                        printed_destination["temp"] == n || in_extra,
                        "{here}: temporary R{n}"
                    );
                    doubles += usize::from(in_extra && printed_destination.get("out").is_some());
                }
                Destination::Output(o) => {
                    assert_eq!(printed_destination["out"], o.field(), "{here}: output");
                }
                Destination::None => {
                    // a MOVC writes the condition register alone: no temporary, and RPCS3 prints RC as the destination
                    assert!(
                        ins.condition.update && printed_destination["condition_register"] == true,
                        "{here}: no destination, where RPCS3 prints one"
                    );
                }
            }
            let printed_sources = op["sources"].as_array().expect("sources");
            assert_eq!(printed_sources.len(), sources.len(), "{here}: source count");
            for (printed_source, &n) in printed_sources.iter().zip(&sources) {
                let source = ins.sources[n];
                let (kind, index) = match source.kind {
                    SourceKind::Temporary(r) => ("temp", u32::from(r)),
                    SourceKind::Input => ("input", u32::from(ins.input)),
                    SourceKind::Constant => ("constant", u32::from(ins.constant)),
                    SourceKind::None => panic!("{here}: source {n} is unused, where RPCS3 prints it"),
                };
                assert_eq!(printed_source["kind"], kind, "{here}: source {n}");
                assert_eq!(printed_source["index"], index, "{here}: source {n} register");
                assert_eq!(
                    printed_source["swizzle"],
                    serde_json::json!(source.swizzle),
                    "{here}: source {n} swizzle"
                );
                assert_eq!(
                    printed_source["negate"], source.negate,
                    "{here}: source {n} negate"
                );
                assert_eq!(
                    printed_source["absolute"], source.absolute,
                    "{here}: source {n} absolute"
                );
                if source.kind == SourceKind::Constant {
                    assert_eq!(
                        !printed_source["relative"].is_null(),
                        ins.constant_relative,
                        "{here}: source {n} relative"
                    );
                }
            }
        }
    }
    assert_eq!(
        at,
        operations.len(),
        "{label}: RPCS3 printed more operations than the program has"
    );
    (at, doubles)
}

fn archives(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let p = entry.expect("directory entry").path();
        if p.is_dir() {
            archives(&p, out);
        } else if p.extension().is_some_and(|s| s.eq_ignore_ascii_case("pak")) {
            out.push(p);
        }
    }
}

#[test]
fn every_vertex_program_agrees_with_its_header() {
    let Some(root) = install(Game::Shred) else { return };
    let Some(references) = oracle(OracleSet::Rpcs3, Game::Shred, "vertex") else {
        return;
    };
    let mut paths = Vec::new();
    archives(&root.join("PS3_GAME/USRDIR"), &mut paths);
    paths.sort();
    expect_found(Query::Install(Game::Shred, None), "PAKs", paths.len());
    let depth = depth();
    if depth == Depth::Smoke {
        let n = paths.len();
        paths.rotate_left(seed() as usize % n);
    }
    let mut seen: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let (mut shader_archives, mut occurrences, mut instructions) = (0, 0, 0usize);
    let mut vector: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut scalar: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut outputs: BTreeSet<u8> = BTreeSet::new();
    let (mut constant_over_255, mut relative, mut unassigned, mut abs_or_neg) =
        (0usize, 0usize, 0usize, 0usize);
    let mut condition_updates = 0usize;
    let (mut compared, mut doubles) = (0usize, 0usize);
    for path in &paths {
        if depth == Depth::Smoke && shader_archives >= 8 {
            break;
        }
        let pak = Pak::open(path).unwrap();
        let mut found = false;
        for (i, entry) in pak.entries().iter().enumerate() {
            if !entry.name.ends_with(b".vsp") {
                continue;
            }
            found = true;
            let bytes = pak.payload(i).unwrap();
            let mut offset = 0;
            while offset < bytes.len() {
                let label = format!(
                    "{} {:?} at {offset}",
                    path.display(),
                    String::from_utf8_lossy(&entry.name)
                );
                let p = parse_prefix(&bytes[offset..]).unwrap_or_else(|e| panic!("{label}: {e}"));
                offset = (offset + p.bytes.len()).checked_add(63).unwrap() & !63;
                occurrences += 1;
                if let Some(n) = seen.get_mut(p.bytes) {
                    *n += 1;
                    continue;
                }
                seen.insert(p.bytes.to_vec(), 1);
                let Stage::Vertex(header) = &p.stage else {
                    panic!("{label}: not a vertex program")
                };
                let program = vertex::decode(p.microcode).unwrap_or_else(|e| panic!("{label}: {e}"));
                // the header agrees with what the instructions do
                assert_eq!(
                    program.instructions.len(),
                    header.instruction_count as usize,
                    "{label}: instruction count"
                );
                assert_eq!(header.instruction_slot, 0, "{label}: slot");
                assert_eq!(
                    program.register_count(),
                    header.register_count,
                    "{label}: registers"
                );
                assert_eq!(
                    program.input_mask(),
                    Some(header.attribute_input_mask),
                    "{label}: attributes read"
                );
                assert_eq!(
                    program.output_mask(),
                    header.attribute_output_mask,
                    "{label}: outputs written"
                );
                assert_eq!(header.user_clip_mask, 0, "{label}: clip planes");
                let hash: String = Sha256::digest(p.microcode)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let reference: Value =
                    serde_json::from_slice(&std::fs::read(references.join(format!("{hash}.json"))).unwrap())
                        .unwrap();
                let (n, d) = compare(&program, &reference, &format!("{label} {hash}"));
                compared += n;
                doubles += d;
                for ins in &program.instructions {
                    instructions += 1;
                    *vector.entry(ins.vector.mnemonic()).or_default() += 1;
                    *scalar.entry(ins.scalar.mnemonic()).or_default() += 1;
                    outputs.extend(ins.outputs().iter().map(|o| o.field()));
                    constant_over_255 += usize::from(ins.constant > 255);
                    relative += usize::from(ins.input_relative || ins.constant_relative);
                    unassigned += usize::from(ins.unassigned != [0; 4]);
                    abs_or_neg += usize::from(ins.sources.iter().any(|s| s.negate || s.absolute));
                    condition_updates += usize::from(ins.condition.update);
                }
            }
            assert_eq!(offset, bytes.len(), "{}", path.display());
        }
        shader_archives += usize::from(found);
    }
    expect_found(Query::Install(Game::Shred, None), "vertex programs", occurrences);
    eprintln!(
        "SHRED vertex {depth:?}: occurrences={occurrences} distinct={} instructions={instructions} outputs={outputs:?} \
         constants>255={constant_over_255} relative={relative} unassigned={unassigned} negated_or_absolute={abs_or_neg} \
         condition_updates={condition_updates} operations compared with RPCS3={compared} (double destinations \
         {doubles})\n  vector={vector:?}\n  scalar={scalar:?}",
        seen.len()
    );
    if depth == Depth::Full {
        assert_eq!((occurrences, seen.len()), (199017, 170));
    }
}
