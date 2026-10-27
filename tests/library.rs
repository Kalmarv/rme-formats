//! Hand-worked bank identities: which bank holds a material id, and the namespace rule for id zero.
use rme_formats::{
    Error,
    library::{BankEvidence, MaterialLibrary},
    material::{MaterialKey, MaterialSampler, ShredMaterial},
    pak::Pak,
    texture,
};

fn material(bank: &str, id: u32, paths: &[&str]) -> ShredMaterial {
    ShredMaterial {
        key: MaterialKey {
            bank: bank.into(),
            id,
        },
        unknown_header: [0; 4],
        vertex_shader: "v".into(),
        fragment_shader: "p".into(),
        parameters: vec![],
        samplers: paths
            .iter()
            .enumerate()
            .map(|(i, path)| MaterialSampler {
                slot: i as u32,
                hash: 999,
                name: None,
                name_source: None,
                texture: (*path).into(),
                unknown_state: [0; 5],
            })
            .collect(),
    }
}

#[test]
fn unique_ids_and_namespace_inference_keep_all_candidates() {
    let library = MaterialLibrary::from_materials(
        vec![
            material("environments", 7, &[]),
            material("environments", 0, &[]),
            material("characters", 0, &[]),
            material("actors", 0, &[]),
        ],
        &[],
    )
    .unwrap();
    let r = library.resolve(7, None);
    assert_eq!(r.evidence, Some(BankEvidence::UniqueId));
    assert_eq!(r.selected.unwrap().bank, "environments");
    for (prefix, bank) in [
        ("levels", "environments"),
        ("actors", "actors"),
        ("characters", "characters"),
    ] {
        let r = library.resolve(0, Some(prefix.as_bytes()));
        assert_eq!(r.candidates.len(), 3);
        assert_eq!(r.selected.unwrap().bank, bank);
        assert_eq!(r.evidence, Some(BankEvidence::InferredQuartetNamespace));
    }
    let r = library.resolve(0, Some(b"unknown"));
    assert_eq!(r.candidates.len(), 3);
    assert!(r.selected.is_none());
    assert!(r.evidence.is_none());
    assert!(library.resolve(99, None).candidates.is_empty());
    assert_eq!(
        library
            .material(&MaterialKey {
                bank: "actors".into(),
                id: 0
            })
            .map(|m| m.key.id),
        Some(0)
    );
    assert!(
        MaterialLibrary::from_materials(vec![material("actors", 1, &[]), material("actors", 1, &[])], &[])
            .is_err()
    );
}

/// A stored PAK of named literal entries (no sections), to hang hostile entries on.
fn stored_pak(rows: &[(&[u8], &[u8])]) -> Pak {
    let mut names = Vec::new();
    let mut name_offsets = Vec::new();
    for (name, _) in rows {
        name_offsets.push(names.len());
        names.extend(*name);
        names.push(0);
    }
    let mut pos = 16 + 32 * rows.len() + names.len();
    let mut out = b"PAK\0\xfe\xeb\0\x04".to_vec();
    out.extend((rows.len() as u32).to_be_bytes());
    out.extend((names.len() as u32).to_be_bytes());
    for ((name, data), name_offset) in rows.iter().zip(name_offsets) {
        for n in [
            primase_foundations::hash::sdbm(name),
            name_offset as u32,
            0,
            16,
            pos as u32,
            data.len() as u32,
            0,
            0,
        ] {
            out.extend(n.to_be_bytes());
        }
        pos += data.len();
    }
    out.extend(names);
    for (_, data) in rows {
        out.extend_from_slice(data);
    }
    Pak::from_bytes(out).expect("fixture PAK")
}

#[test]
fn a_bad_texture_header_keeps_its_own_offset_and_variant() {
    // 84 zero bytes: the first constant (0xafaf0003 at +0) is wrong
    let assets = stored_pak(&[(b"art/bad.thp", &[0; 84])]);
    let error = MaterialLibrary::from_materials(vec![], &[("assets", &assets)]).unwrap_err();
    assert!(
        matches!(&error, Error::Texture(inner) if matches!(**inner, texture::Error::Header { offset: 0, .. })),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "texture header at 0x0: header constant mismatch"
    );
}

#[test]
fn a_program_name_that_is_not_text_is_reported_at_its_entry() {
    let assets = stored_pak(&[(b"\xff\xfe.psp", &[])]);
    let at = assets.entries()[0].offset;
    let banks = stored_pak(&[]);
    let error = MaterialLibrary::new(&banks, &[("assets", &assets)]).unwrap_err();
    let Error::Invalid { offset, .. } = error else {
        panic!("{error:?}")
    };
    assert_eq!(offset, at);
}
