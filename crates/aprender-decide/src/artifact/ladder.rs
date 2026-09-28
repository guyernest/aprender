//! FALSIFY-DECIDE-APR-001/-008/-009/-011/-013: every ladder rung refuses its own induced
//! negative, naming the rung, and every manifest leaf is bound to its sha-bound source. Negatives are induced by editing the PACKED bytes (or by
//! packing a mutated in-memory [`PackInputs`]) — never by weakening a rung.

use super::tests::pack_tiny;
use super::{
    check_index_extent, check_size, expected_bytes, load_verified, load_verified_within,
    read_bounded_within, read_decide_apr_bytes_bounded, write_decide_apr_within, ArtifactError,
    ArtifactLimits, CUSTOM_METADATA_KEY, MAX_ARTIFACT_BYTES, MAX_METADATA_BYTES, MAX_TENSOR_COUNT,
    MIN_INDEX_ENTRY_BYTES, PROBE_MAX_ROW_TOKENS,
};
use crate::pack::PackInputs;
use crate::test_support::fixture_dir;
use aprender::format::v2::{
    AprV2Header, AprV2Metadata, AprV2ReaderRef, AprV2Writer, TensorDType, HEADER_SIZE_V2,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type Tensors = Vec<(String, TensorDType, Vec<usize>, Vec<u8>)>;

/// The packed tiny fixture, packed once per test binary.
fn packed() -> Vec<u8> {
    static PACKED: OnceLock<Vec<u8>> = OnceLock::new();
    PACKED.get_or_init(pack_tiny).clone()
}

fn inputs() -> PackInputs {
    let dir = fixture_dir();
    PackInputs::from_run_dir(&dir, &dir.join("data")).expect("the tiny run dir reads")
}

/// The ladder's refusal of `bytes` (panics if the ladder ACCEPTS an induced negative).
fn refuse(bytes: &[u8]) -> ArtifactError {
    load_verified(bytes).expect_err("the ladder accepted an induced negative")
}

/// Re-write `bytes` through the container writer after `edit`.
fn repack(bytes: &[u8], edit: impl FnOnce(&mut AprV2Metadata, &mut Tensors)) -> Vec<u8> {
    let r = AprV2ReaderRef::from_bytes(bytes).expect("open the packed artifact");
    let mut meta = r.metadata().clone();
    let mut tensors: Tensors = r
        .tensor_index()
        .iter()
        .map(|e| {
            let data = r.get_tensor_data(&e.name).expect("tensor data");
            (e.name.clone(), e.dtype, e.shape.clone(), data.to_vec())
        })
        .collect();
    edit(&mut meta, &mut tensors);
    let mut w = AprV2Writer::new(meta);
    for (name, dtype, shape, data) in tensors {
        w.add_tensor(name, dtype, shape, data);
    }
    w.write().expect("repack")
}

/// Edit the manifest JSON and re-write the container.
fn edit_manifest(bytes: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    repack(bytes, |meta, _| {
        let text = meta.custom[CUSTOM_METADATA_KEY]
            .as_str()
            .expect("manifest string")
            .to_string();
        let mut v: serde_json::Value = serde_json::from_str(&text).expect("manifest JSON");
        edit(&mut v);
        let text = serde_json::to_string(&v).expect("manifest re-serializes");
        meta.custom.insert(
            CUSTOM_METADATA_KEY.to_string(),
            serde_json::Value::String(text),
        );
    })
}

/// Edit one tensor's bytes in place.
fn edit_tensor(bytes: &[u8], name: &str, edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut edit = Some(edit);
    repack(bytes, |_, tensors| {
        let t = tensors
            .iter_mut()
            .find(|t| t.0 == name)
            .unwrap_or_else(|| panic!("tensor {name}"));
        if let Some(f) = edit.take() {
            f(&mut t.3);
        }
    })
}

/// Rewrite the header with `edit` applied and a VALID CRC (a forged header).
fn forge_header(bytes: &mut [u8], edit: impl FnOnce(&mut AprV2Header)) {
    let mut h = AprV2Header::from_bytes(bytes).expect("header parses");
    edit(&mut h);
    h.update_checksum();
    bytes[..HEADER_SIZE_V2].copy_from_slice(&h.to_bytes());
}

fn header(bytes: &[u8]) -> AprV2Header {
    AprV2Header::from_bytes(bytes).expect("header parses")
}

/// A reader that must never be touched.
struct Untouchable;

impl std::io::Read for Untouchable {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("the reader was touched before the declared length was checked")
    }
}

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

/// The repack helper itself changes nothing: the unedited re-write is byte-identical
/// and loads, so every negative below is caused by its edit alone.
#[test]
fn repack_control_loads() {
    let b = packed();
    let r = repack(&b, |_, _| {});
    assert_eq!(r, b, "repack without an edit is byte-identical");
    load_verified(&r).expect("the control loads");
}

// ---------------------------------------------------------------------------
// Rung 1: bounded read
// ---------------------------------------------------------------------------

#[test]
fn declared_length_over_cap() {
    let e = read_decide_apr_bytes_bounded(Untouchable, Some(MAX_ARTIFACT_BYTES + 1))
        .expect_err("over-cap declared length");
    assert_eq!(
        e,
        ArtifactError::ArtifactTooLarge {
            what: "declared_length",
            observed: MAX_ARTIFACT_BYTES + 1,
            cap: MAX_ARTIFACT_BYTES,
        }
    );
    assert_eq!(e.rung(), "1 bounded_read");
}

#[test]
fn read_over_cap() {
    let b = packed();
    let n = b.len() as u64;
    // Exactly the cap is a legal artifact.
    let at_cap = ArtifactLimits::tiny(n, PROBE_MAX_ROW_TOKENS);
    let ok = read_bounded_within(&b[..], Some(n), &at_cap).expect("exactly the cap reads");
    assert_eq!(ok, b);
    // A stream whose length lies (declared 0) is cut at cap + 1 and refused.
    let under = ArtifactLimits::tiny(n - 1, PROBE_MAX_ROW_TOKENS);
    let e = read_bounded_within(&b[..], Some(0), &under).expect_err("one byte over");
    assert_eq!(
        e,
        ArtifactError::ArtifactTooLarge {
            what: "stream",
            observed: n,
            cap: n - 1,
        }
    );
    // An endless stream reads at most cap + 1 bytes.
    let e = read_bounded_within(std::io::repeat(7), None, &under).expect_err("endless");
    assert_eq!(
        e,
        ArtifactError::ArtifactTooLarge {
            what: "stream",
            observed: n,
            cap: n - 1,
        }
    );
}

#[test]
fn in_memory_over_cap() {
    let b = packed();
    let n = b.len() as u64;
    let e = load_verified_within(&b, &ArtifactLimits::tiny(n - 1, PROBE_MAX_ROW_TOKENS))
        .expect_err("in-memory bytes over the cap");
    assert_eq!(
        e,
        ArtifactError::ArtifactTooLarge {
            what: "input_bytes",
            observed: n,
            cap: n - 1,
        }
    );
}

// ---------------------------------------------------------------------------
// Rung 2: header and index extent, before the reader
// ---------------------------------------------------------------------------

#[test]
fn header_crc_flip() {
    let mut b = packed();
    b[20] ^= 0x01; // metadata_size, inside the CRC-covered header
    let e = refuse(&b);
    assert_eq!(e, ArtifactError::HeaderChecksum);
    assert_eq!(e.rung(), "2 header_and_index_extent");
}

/// A forged header (valid CRC) declaring 4097 tensors is refused at rung 2. The
/// container reader, handed the same bytes, fails differently — so the refusal
/// provably came from rung 2, before any index parser ran.
#[test]
fn tensor_count_over_cap() {
    let mut b = packed();
    forge_header(&mut b, |h| h.tensor_count = MAX_TENSOR_COUNT + 1);
    assert_eq!(
        refuse(&b),
        ArtifactError::TensorCountOverCap {
            declared: MAX_TENSOR_COUNT + 1,
            cap: MAX_TENSOR_COUNT,
        }
    );
    assert!(
        AprV2ReaderRef::from_bytes(&b).is_err(),
        "the reader would have refused differently"
    );
}

#[test]
fn index_extent_too_small() {
    let mut b = packed();
    let h = header(&b);
    let extent = h.data_offset - h.tensor_index_offset;
    let declared = u32::try_from(extent / MIN_INDEX_ENTRY_BYTES + 1).expect("fits u32");
    assert!(declared <= MAX_TENSOR_COUNT, "the count is under the cap");
    forge_header(&mut b, |h| h.tensor_count = declared);
    assert_eq!(
        refuse(&b),
        ArtifactError::IndexExtentTooSmall { declared, extent }
    );
}

#[test]
fn index_past_end() {
    let mut b = packed();
    let len = b.len() as u64;
    forge_header(&mut b, |h| h.data_offset = len + 1);
    assert_eq!(
        refuse(&b),
        ArtifactError::IndexPastEnd {
            data_offset: len + 1,
            file_len: len,
        }
    );
}

/// Replace blob `name` with `new` AND re-pin its manifest sha256 — the artifact's author
/// controls both, so the blob hash alone is not a bound on what the blob declares.
fn swap_blob(bytes: &[u8], name: &str, new: Vec<u8>) -> Vec<u8> {
    let sha = crate::pack::sha256_hex(&new);
    let swapped = edit_tensor(bytes, name, |data| *data = new);
    edit_manifest(&swapped, |m| {
        for blob in m["blobs"].as_array_mut().expect("manifest blobs") {
            if blob["name"] == name {
                blob["sha256"] = serde_json::Value::String(sha.clone());
            }
        }
    })
}

/// A self-consistent artifact whose config blobs declare astronomically many layers is a
/// typed rung-4 refusal, never an allocation sized by the declared count (a 2^62-layer
/// encoder config used to abort the process inside the config parse).
#[test]
fn untrusted_layer_counts_are_bounded_before_derivation() {
    let b = packed();
    let r = AprV2ReaderRef::from_bytes(&b).expect("open the packed artifact");
    let blob_json = |name: &str| -> serde_json::Value {
        serde_json::from_slice(r.get_tensor_data(name).expect("blob")).expect("blob JSON")
    };
    let mut enc = blob_json(super::ENCODER_CONFIG_BLOB);
    enc.as_object_mut()
        .expect("encoder config object")
        .remove("layer_types");
    enc["num_hidden_layers"] = serde_json::Value::from(1u64 << 62);
    let mut agent = blob_json(super::AGENT_CONFIG_BLOB);
    agent["head_layers"] = serde_json::Value::from(1u64 << 40);
    for (blob, value) in [
        (super::ENCODER_CONFIG_BLOB, enc),
        (super::AGENT_CONFIG_BLOB, agent),
    ] {
        let forged = swap_blob(&b, blob, serde_json::to_vec(&value).expect("serialize"));
        let e = refuse(&forged);
        assert!(
            matches!(&e, ArtifactError::ConfigBlob { blob: got, .. } if *got == blob),
            "{blob}: {e}"
        );
        assert_eq!(e.rung(), "4 structural", "{blob}");
    }
}

/// A forged header (valid CRC) declaring a metadata section over the cap is refused at
/// rung 2, before the container reader would parse that many bytes into a JSON tree.
#[test]
fn metadata_over_cap() {
    let mut b = packed();
    forge_header(&mut b, |h| h.metadata_size = MAX_METADATA_BYTES + 1);
    let e = refuse(&b);
    assert_eq!(
        e,
        ArtifactError::MetadataOverCap {
            declared: MAX_METADATA_BYTES + 1,
            cap: MAX_METADATA_BYTES,
        }
    );
    assert_eq!(e.rung(), "2 header_and_index_extent");
}

/// KANI-DECIDE-APR-001's evidence: the rung-2 predicate over every combination of edge
/// values, including overflow edges, against a u128 reference.
#[test]
fn rung2_predicate_exhaustive() {
    let counts = [0u32, 1, 2, 63, 4095, 4096, 4097, u32::MAX / 20, u32::MAX];
    let offsets = [
        0u64,
        1,
        19,
        20,
        64,
        4096,
        81_920,
        u64::MAX / 20,
        u64::MAX - 1,
        u64::MAX,
    ];
    let mut checked = 0usize;
    for &count in &counts {
        for &tio in &offsets {
            for &dof in &offsets {
                for &len in &offsets {
                    let want = count <= MAX_TENSOR_COUNT
                        && dof <= len
                        && dof >= tio
                        && u128::from(count) * u128::from(MIN_INDEX_ENTRY_BYTES)
                            <= u128::from(dof - tio.min(dof));
                    let got = check_index_extent(count, tio, dof, len).is_ok();
                    assert_eq!(got, want, "count {count} tio {tio} dof {dof} len {len}");
                    checked += 1;
                }
            }
        }
    }
    println!("rung2_predicate_exhaustive: {checked} cases");
}

// ---------------------------------------------------------------------------
// Rung 3: manifest
// ---------------------------------------------------------------------------

#[test]
fn wrong_model_type() {
    let b = repack(&packed(), |meta, _| meta.model_type = "setfit".to_string());
    assert_eq!(
        refuse(&b),
        ArtifactError::WrongModelType {
            observed: "setfit".to_string(),
        }
    );
}

#[test]
fn two_custom_keys() {
    let b = repack(&packed(), |meta, _| {
        meta.custom
            .insert("extra".to_string(), serde_json::Value::from("x"));
    });
    assert_eq!(
        refuse(&b),
        ArtifactError::CustomKeys {
            observed: vec!["decide".to_string(), "extra".to_string()],
        }
    );
}

#[test]
fn unknown_manifest_key() {
    let b = edit_manifest(&packed(), |v| v["extra"] = serde_json::Value::from(1));
    let e = refuse(&b);
    assert!(
        matches!(&e, ArtifactError::ManifestParse { reason } if reason.contains("extra")),
        "{e}"
    );
    assert_eq!(e.rung(), "3 manifest");
}

#[test]
fn schema_version_2() {
    let b = edit_manifest(&packed(), |v| {
        v["schema_version"] = serde_json::Value::from(2);
    });
    assert_eq!(refuse(&b), ArtifactError::SchemaVersion { observed: 2 });
}

#[test]
fn unknown_method() {
    let b = edit_manifest(&packed(), |v| v["method"] = serde_json::Value::from("kev"));
    assert_eq!(
        refuse(&b),
        ArtifactError::UnknownMethod {
            observed: "kev".to_string(),
        }
    );
}

// ---------------------------------------------------------------------------
// Rung 4: structure
// ---------------------------------------------------------------------------

#[test]
fn missing_tensor() {
    let b = repack(&packed(), |_, t| t.retain(|t| t.0 != "scorer.3.bias"));
    let e = refuse(&b);
    assert_eq!(
        e,
        ArtifactError::MissingTensor {
            name: "scorer.3.bias".to_string(),
        }
    );
    assert_eq!(e.rung(), "4 structural");
}

#[test]
fn extra_tensor() {
    let b = repack(&packed(), |_, t| {
        t.push((
            "rogue.weight".to_string(),
            TensorDType::F16,
            vec![1],
            vec![0, 0],
        ));
    });
    assert_eq!(
        refuse(&b),
        ArtifactError::UnexpectedTensor {
            name: "rogue.weight".to_string(),
        }
    );
}

#[test]
fn size_mismatch() {
    let b = repack(&packed(), |_, t| {
        let e = t
            .iter_mut()
            .find(|t| t.0 == "type_emb.weight")
            .expect("type_emb");
        e.2 = vec![3, 31];
    });
    assert_eq!(
        refuse(&b),
        ArtifactError::SizeMismatch {
            name: "type_emb.weight".to_string(),
            expected: 3 * 31 * 2,
            observed: 3 * 32 * 2,
        }
    );
}

/// KANI-DECIDE-APR-003's evidence: the rung-4 size rule over bounded shapes of length
/// <= 4 for every carried dtype, against a u128 reference, including overflow.
#[test]
fn size_rule_exhaustive() {
    let dims = [0usize, 1, 2, 3, 7, 1 << 32, usize::MAX];
    let dtypes = [
        (TensorDType::F16, 2u128),
        (TensorDType::F32, 4),
        (TensorDType::U8, 1),
    ];
    let mut shapes: Vec<Vec<usize>> = vec![vec![]];
    for rank in 1..=4usize {
        let mut next = Vec::new();
        for s in shapes.iter().filter(|s| s.len() == rank - 1) {
            for &d in &dims {
                let mut t = s.clone();
                t.push(d);
                next.push(t);
            }
        }
        shapes.extend(next);
    }
    let mut checked = 0usize;
    for shape in &shapes {
        for &(dtype, width) in &dtypes {
            // The exact product: 0 whenever any dimension is 0, else the u128 product,
            // which exceeds u64 (-> None) whenever it overflows u128.
            let reference = if shape.contains(&0) {
                Some(0)
            } else {
                shape
                    .iter()
                    .try_fold(width, |a, &d| a.checked_mul(d as u128))
                    .and_then(|v| u64::try_from(v).ok())
            };
            assert_eq!(
                expected_bytes(shape, dtype),
                reference,
                "{shape:?} {dtype:?}"
            );
            if let Some(size) = reference {
                assert!(check_size("t", shape, dtype, size).is_ok());
                assert!(check_size("t", shape, dtype, size.wrapping_add(1)).is_err());
            } else {
                assert!(check_size("t", shape, dtype, 0).is_err());
                assert!(check_size("t", shape, dtype, u64::MAX).is_err());
            }
            checked += 1;
        }
    }
    assert_eq!(
        expected_bytes(&[2], TensorDType::BF16),
        None,
        "no other dtype"
    );
    println!("size_rule_exhaustive: {checked} cases");
}

#[test]
fn tokenizer_blob_hash() {
    let b = edit_tensor(&packed(), "tokenizer.blob", |d| d[0] ^= 0x01);
    assert_eq!(
        refuse(&b),
        ArtifactError::BlobHashMismatch {
            blob: "tokenizer.blob".to_string(),
        }
    );
}

#[test]
fn task_blob_hash() {
    let b = edit_tensor(&packed(), "decide.task_json", |d| d[0] ^= 0x01);
    assert_eq!(
        refuse(&b),
        ArtifactError::BlobHashMismatch {
            blob: "decide.task_json".to_string(),
        }
    );
}

/// The contract's qa_gate falsification: one flipped byte of the recipe blob is refused
/// at rung 4 naming the blob (so recipe_id can never describe other bytes).
#[test]
fn recipe_blob_hash() {
    let b = edit_tensor(&packed(), "decide.recipe_json", |d| {
        let last = d.len() - 1;
        d[last] ^= 0x01;
    });
    assert_eq!(
        refuse(&b),
        ArtifactError::BlobHashMismatch {
            blob: "decide.recipe_json".to_string(),
        }
    );
}

/// WR-01, decide side: the rung-4 repeat walk names the first repeated name whatever the
/// order of the index — an adjacent repeat, a non-adjacent one — and `None` when every
/// name is unique. This is the red-side proof of the rung-4 logic: while plan 08-20's
/// reader refuses a repeated name at rung 3, no real artifact reaches the rung-4 call.
#[test]
fn repeated_name_walk_names_the_first_repeat() {
    assert_eq!(
        super::first_repeated_tensor_name(["a", "b", "b", "c"]),
        Some("b"),
        "adjacent repeat"
    );
    assert_eq!(
        super::first_repeated_tensor_name(["a", "b", "c", "a"]),
        Some("a"),
        "non-adjacent repeat (not visible to a sorted-neighbour check)"
    );
    assert_eq!(
        super::first_repeated_tensor_name(["b", "a", "b", "a"]),
        Some("b"),
        "the FIRST repeat in index order"
    );
    assert_eq!(super::first_repeated_tensor_name(["a", "b", "c"]), None);
    assert_eq!(super::first_repeated_tensor_name(Vec::<&str>::new()), None);
}

/// WR-01, decide side: an artifact whose index names one encoder weight twice (the writer
/// wrote both entries) is refused at load through the stdio door, and the refusal names the
/// tensor. While plan 08-20's reader change stands the refusal is the container's (rung 3,
/// `duplicate tensor name ...`); rung 4's `DuplicateTensor` is the decide-side backstop.
#[test]
fn duplicate_tensor_name_is_refused_at_load() {
    let b = packed();
    let name = AprV2ReaderRef::from_bytes(&b)
        .expect("open the packed artifact")
        .tensor_index()
        .iter()
        .map(|e| e.name.clone())
        .find(|n| n.starts_with("encoder."))
        .expect("an encoder weight");
    let dup = repack(&b, |_, tensors| {
        let copy = tensors
            .iter()
            .find(|t| t.0 == name)
            .cloned()
            .expect("the weight to duplicate");
        tensors.push(copy);
    });
    assert_eq!(
        header(&dup).tensor_count,
        header(&b).tensor_count + 1,
        "the writer wrote both entries"
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("duplicate-name.apr");
    std::fs::write(&path, &dup).expect("write the forged artifact");
    let e = crate::Decider::load_path(&path).expect_err("a repeated tensor name loaded");
    match &e {
        ArtifactError::DuplicateTensor { name: got } => {
            assert_eq!(got, &name, "{e}");
            assert_eq!(e.rung(), "4 structural");
        }
        ArtifactError::Container { reason } => {
            assert!(
                reason.contains("duplicate") && reason.contains(&name),
                "the container refusal must name the repeated tensor: {e}"
            );
            assert_eq!(e.rung(), "3 manifest");
        }
        other => panic!("refused for another reason: {other}"),
    }
    println!(
        "duplicate_tensor_name_is_refused_at_load: {name} refused at rung {}: {e}",
        e.rung()
    );
}

/// `inspect` prints identity only after rungs 1-4, so a manifest whose base its own recipe
/// blob contradicts is refused by `inspect_manifest` too (IN-02, A2-3); the packed control
/// inspects.
#[test]
fn inspect_refuses_a_manifest_its_blobs_contradict() {
    super::inspect_manifest(&packed()).expect("control: the packed artifact inspects");
    let b = edit_manifest(&packed(), |v| {
        v["base"]["revision"] = serde_json::Value::from("0123456789abcdef0123456789abcdef01234567");
    });
    let e = super::inspect_manifest(&b).expect_err("inspect printed a contradicted identity");
    assert_eq!(
        e,
        ArtifactError::ManifestDisagreesWithBlob { field: "base" }
    );
    assert_eq!(e.rung(), "4 structural");
}

#[test]
fn labels_disagree_with_task() {
    let b = edit_manifest(&packed(), |v| {
        let labels = v["labels"].as_array_mut().expect("labels");
        labels.reverse();
    });
    assert_eq!(
        refuse(&b),
        ArtifactError::LabelsDisagreeWithTask {
            manifest: vec!["account".into(), "billing".into(), "shipping".into()],
            task: vec!["shipping".into(), "billing".into(), "account".into()],
        }
    );
}

// ---------------------------------------------------------------------------
// Rung 4 (e): manifest bindings (decide-apr-v1 manifest.bindings, plan 08-19)
// ---------------------------------------------------------------------------

/// `manifest.base` is minted into `ModelIdentity.base` (served as `model.base` in every
/// classify response): a revision that is not the embedded recipe blob's is refused at
/// rung 4, and so is a variant the recipe does not declare.
#[test]
fn manifest_base_disagrees_with_recipe_blob() {
    let b = edit_manifest(&packed(), |v| {
        v["base"]["revision"] = serde_json::Value::from("0123456789abcdef0123456789abcdef01234567");
    });
    let e = refuse(&b);
    assert_eq!(
        e,
        ArtifactError::ManifestDisagreesWithBlob { field: "base" }
    );
    assert_eq!(e.rung(), "4 structural");

    let b = edit_manifest(&packed(), |v| {
        v["variant"] = serde_json::Value::from("production");
    });
    let e = refuse(&b);
    assert_eq!(
        e,
        ArtifactError::ManifestDisagreesWithBlob { field: "variant" }
    );
    assert_eq!(e.rung(), "4 structural");
}

/// The same crafted bytes are refused through every door a server loads by: the in-memory
/// door, the pre-hashed door the Lambda cold start uses, and the path door the stdio server
/// uses — none of them needs `verify` to see it.
#[test]
fn manifest_bound_at_every_load_door() {
    let want = ArtifactError::ManifestDisagreesWithBlob { field: "base" };
    let b = edit_manifest(&packed(), |v| {
        v["base"]["checkpoint"] = serde_json::Value::from("en-root");
    });
    let bytes_door = crate::Decider::load_bytes(&b).expect_err("load_bytes accepted a forged base");
    assert_eq!(bytes_door, want, "load_bytes");
    let hashed_door = crate::Decider::load_hashed(&super::HashedArtifact::new(&b))
        .expect_err("load_hashed accepted a forged base");
    assert_eq!(hashed_door, want, "load_hashed (the Lambda door)");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("forged-base.apr");
    std::fs::write(&path, &b).expect("write the forged artifact");
    let path_door = crate::Decider::load_path(&path).expect_err("load_path accepted a forged base");
    assert_eq!(path_door, want, "load_path (the stdio door)");
    // Control: the unforged bytes load through the same three doors.
    let ok = packed();
    crate::Decider::load_bytes(&ok).expect("control: load_bytes");
    crate::Decider::load_hashed(&super::HashedArtifact::new(&ok)).expect("control: load_hashed");
    let ok_path = dir.path().join("control.apr");
    std::fs::write(&ok_path, &ok).expect("write the control artifact");
    crate::Decider::load_path(&ok_path).expect("control: load_path");
}

// ---------------------------------------------------------------------------
// Rung 4 (e): every manifest leaf, bound (plan 08-19 Task 2)
// ---------------------------------------------------------------------------

/// JSON-pointer edits: set each pointer to its value.
type Edits = Vec<(String, Value)>;

/// A COHERENT forgery. The artifact's author controls every blob and every hash, so a
/// forger who edits the recipe or the gate report also re-pins what hashes them: the
/// recipe's digest into `manifest.blobs`, `manifest.recipe_id` and the report's `recipe_id`;
/// the report's digest into `manifest.blobs` and `manifest.gate.report_sha256`. Explicit
/// report edits land after the recipe re-pin and explicit manifest edits land last, so an
/// edit always wins over a re-pin. With no edits the bytes are unchanged.
fn forge(
    bytes: &[u8],
    recipe: &[(String, Value)],
    report: &[(String, Value)],
    manifest: &[(String, Value)],
) -> Vec<u8> {
    let set = |v: &mut Value, edits: &[(String, Value)]| {
        for (ptr, new) in edits {
            *v.pointer_mut(ptr)
                .unwrap_or_else(|| panic!("forge: no field {ptr}")) = new.clone();
        }
    };
    let r = AprV2ReaderRef::from_bytes(bytes).expect("open the packed artifact");
    let blob = |name: &str| r.get_tensor_data(name).expect("blob").to_vec();
    let mut recipe_bytes = blob(super::RECIPE_BLOB);
    let mut report_bytes = blob(super::GATE_REPORT_BLOB);
    let mut pins = Edits::new();
    let mut report_edits = Edits::new();
    if !recipe.is_empty() {
        let mut v: Value = serde_json::from_slice(&recipe_bytes).expect("recipe JSON");
        set(&mut v, recipe);
        recipe_bytes = serde_json::to_vec(&v).expect("recipe re-serializes");
        let sha = Value::from(crate::pack::sha256_hex(&recipe_bytes));
        pins.push(("/blobs/4/sha256".into(), sha.clone()));
        pins.push(("/recipe_id".into(), sha.clone()));
        report_edits.push(("/recipe_id".into(), sha));
    }
    report_edits.extend(report.iter().cloned());
    if !report_edits.is_empty() {
        let mut v: Value = serde_json::from_slice(&report_bytes).expect("report JSON");
        set(&mut v, &report_edits);
        report_bytes = serde_json::to_vec(&v).expect("report re-serializes");
        let sha = Value::from(crate::pack::sha256_hex(&report_bytes));
        pins.push(("/blobs/5/sha256".into(), sha.clone()));
        pins.push(("/gate/report_sha256".into(), sha));
    }
    let swapped = repack(bytes, |_, tensors| {
        for t in tensors.iter_mut() {
            if t.0 == super::RECIPE_BLOB {
                t.2 = vec![recipe_bytes.len()];
                t.3 = recipe_bytes.clone();
            } else if t.0 == super::GATE_REPORT_BLOB {
                t.2 = vec![report_bytes.len()];
                t.3 = report_bytes.clone();
            }
        }
    });
    let all: Edits = pins.into_iter().chain(manifest.iter().cloned()).collect();
    if all.is_empty() {
        return swapped;
    }
    edit_manifest(&swapped, |v| set(v, &all))
}

fn edits(pairs: &[(&str, Value)]) -> Edits {
    pairs
        .iter()
        .map(|(p, v)| ((*p).to_string(), v.clone()))
        .collect()
}

/// The packed tiny manifest as JSON.
fn manifest_json(bytes: &[u8]) -> Value {
    let m = super::inspect_manifest(bytes).expect("the packed manifest");
    serde_json::to_value(m).expect("manifest to JSON")
}

/// A hex digest with its first digit changed (same length, still hex).
fn flip_hex(h: &str) -> String {
    let first = if h.starts_with('0') { '1' } else { '0' };
    std::iter::once(first).chain(h.chars().skip(1)).collect()
}

fn is_hex_digest(s: &str) -> bool {
    s.len() >= 8 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The sweep's deterministic mutation of one leaf. A stored probe probability moves at
/// least 0.25, so rung 7 cannot accept it within probe_probabilities_abs.
fn mutate(pattern: &str, v: &Value) -> Value {
    if pattern.ends_with("/probabilities_f32_hex/[*]") {
        let h = v.as_str().expect("probe hex");
        let p = f32::from_bits(u32::from_str_radix(h, 16).expect("probe hex parses"));
        let moved = if p >= 0.5 { p - 0.3 } else { p + 0.3 };
        return Value::from(format!("{:08x}", moved.to_bits()));
    }
    match v {
        Value::String(s) if is_hex_digest(s) => Value::from(flip_hex(s)),
        Value::String(s) => Value::from(format!("{s}x")),
        Value::Bool(b) => Value::from(!b),
        Value::Number(n) if n.is_u64() => Value::from(n.as_u64().expect("u64") + 1),
        Value::Number(n) if n.is_i64() => Value::from(n.as_i64().expect("i64") + 1),
        Value::Number(n) => Value::from(n.as_f64().expect("f64") * 2.0 + 1.0),
        other => panic!("mutate: {pattern} is not a leaf: {other}"),
    }
}

/// Every leaf of `v` as `(pointer, pattern)`, the pattern with array indices as `[*]`.
fn leaves(v: &Value, ptr: &str, pattern: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                leaves(child, &format!("{ptr}/{k}"), &format!("{pattern}/{k}"), out);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                leaves(child, &format!("{ptr}/{i}"), &format!("{pattern}/[*]"), out);
            }
        }
        _ => out.push((ptr.to_string(), pattern.to_string())),
    }
}

/// One `manifest.bindings` row.
struct Binding {
    rung: u64,
    sources: Vec<String>,
}

/// decide-apr-v1 `manifest.bindings`, read from the contract.
fn bindings_table() -> BTreeMap<String, Binding> {
    let c = crate::test_support::contract_yaml("decide-apr-v1.yaml");
    c["manifest"]["bindings"]
        .as_mapping()
        .expect("decide-apr-v1 manifest.bindings")
        .iter()
        .map(|(k, row)| {
            let leaf = k.as_str().expect("binding key").to_string();
            let rung = row["rung"].as_u64().expect("binding rung");
            let sources = row["sources"]
                .as_sequence()
                .unwrap_or_else(|| panic!("{leaf}: sources"))
                .iter()
                .map(|s| s.as_str().expect("source string").to_string())
                .collect();
            (leaf, Binding { rung, sources })
        })
        .collect()
}

/// A settable node of the binding graph: a manifest leaf, or a field of the recipe or
/// gate-report blob. Every other source (`sha256:`, `derived:`) is a FIXED value.
fn settable(node: &str) -> bool {
    ["manifest:", "recipe:", "report:"]
        .iter()
        .any(|p| node.starts_with(p))
}

/// The binding graph over concrete nodes: an edge per (manifest leaf, source).
fn binding_edges(
    leaf_ptrs: &[(String, String)],
    table: &BTreeMap<String, Binding>,
) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for (ptr, pattern) in leaf_ptrs {
        if let Some(row) = table.get(pattern) {
            for s in &row.sources {
                edges.push((format!("manifest:{ptr}"), s.clone()));
            }
        }
    }
    edges
}

/// The settable nodes connected to `start` without crossing `skip`, and whether any of them
/// is also tied to a fixed source (then the component cannot move as one).
fn component(start: &str, edges: &[(String, String)], skip: usize) -> (BTreeSet<String>, bool) {
    let mut seen = BTreeSet::from([start.to_string()]);
    let mut todo = vec![start.to_string()];
    let mut pinned = false;
    while let Some(n) = todo.pop() {
        for (i, (a, b)) in edges.iter().enumerate() {
            if i == skip {
                continue;
            }
            let other = if *a == n {
                b
            } else if *b == n {
                a
            } else {
                continue;
            };
            if !settable(other) {
                pinned = true;
            } else if seen.insert(other.clone()) {
                todo.push(other.clone());
            }
        }
    }
    (seen, pinned)
}

/// A forgery that breaks EXACTLY the one binding `edges[i]`: every other binding still
/// holds. The component of the leaf (without edge i) moves to the mutated value as one; if
/// that component is tied to a fixed source, the source's component moves instead.
fn break_only(
    bytes: &[u8],
    edges: &[(String, String)],
    i: usize,
    pattern: &str,
    old: &Value,
) -> Vec<u8> {
    let (leaf, source) = &edges[i];
    let (comp, pinned) = component(leaf, edges, i);
    let comp = if pinned {
        assert!(
            settable(source),
            "{leaf} <-> {source}: both ends are fixed; no forgery breaks only this binding"
        );
        let (c, p) = component(source, edges, i);
        assert!(
            !p,
            "{leaf} <-> {source}: both components are tied to a fixed source"
        );
        c
    } else {
        comp
    };
    let new = mutate(pattern, old);
    let (mut recipe, mut report, mut manifest) = (Edits::new(), Edits::new(), Edits::new());
    for node in comp {
        let (kind, ptr) = node.split_once(':').expect("node kind");
        let target = match kind {
            "manifest" => &mut manifest,
            "recipe" => &mut recipe,
            "report" => &mut report,
            other => panic!("{node}: {other} is not settable"),
        };
        target.push((ptr.to_string(), new.clone()));
    }
    forge(bytes, &recipe, &report, &manifest)
}

/// `/calibration/t_applied` -> `calibration.t_applied`.
fn dotted(ptr: &str) -> String {
    ptr.trim_start_matches('/').replace('/', ".")
}

fn rung_number(e: &ArtifactError) -> u64 {
    e.rung()
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{e}: not a load rung"))
}

/// The forge itself changes nothing: with no edits the bytes are byte-identical, and a
/// recipe or report re-serialized with every hash re-pinned still loads.
#[test]
fn manifest_forge_control_loads() {
    let b = packed();
    assert_eq!(forge(&b, &[], &[], &[]), b, "no edits: byte-identical");
    let r = forge(
        &b,
        &edits(&[("/seed", Value::from(20_260_925))]),
        &edits(&[("/device_used", Value::from("cpu"))]),
        &[],
    );
    assert_ne!(r, b, "the blobs were re-serialized and re-pinned");
    load_verified(&r).expect("a coherent re-pin of unchanged values loads");
}

fn assert_field(bytes: &[u8], field: &'static str) {
    let e = refuse(bytes);
    assert_eq!(e, ArtifactError::ManifestDisagreesWithBlob { field }, "{e}");
    assert_eq!(e.rung(), "4 structural");
}

/// `manifest.gate.report_sha256` must be the digest of the EMBEDDED gate report.
#[test]
fn manifest_gate_report_sha_disagrees_with_blob() {
    let b = packed();
    let m = manifest_json(&b);
    let flipped = flip_hex(m["gate"]["report_sha256"].as_str().expect("sha"));
    let f = forge(
        &b,
        &[],
        &[],
        &edits(&[("/gate/report_sha256", Value::from(flipped))]),
    );
    assert_field(&f, "gate.report_sha256");
}

/// `inputs_sha256.task_json` / `.tokenizer_json` must be the embedded blobs' digests — even
/// when the embedded report agrees with the manifest's forged value.
#[test]
fn manifest_inputs_disagree_with_blobs() {
    let b = packed();
    let m = manifest_json(&b);
    for (ptr, field) in [
        ("/inputs_sha256/task_json", "inputs_sha256.task_json"),
        (
            "/inputs_sha256/tokenizer_json",
            "inputs_sha256.tokenizer_json",
        ),
    ] {
        let flipped = Value::from(flip_hex(m.pointer(ptr).and_then(Value::as_str).expect(ptr)));
        let f = forge(
            &b,
            &[],
            &edits(&[(ptr, flipped.clone())]),
            &edits(&[(ptr, flipped)]),
        );
        assert_field(&f, field);
    }
}

/// Every `inputs_sha256` field must equal the embedded report's.
#[test]
fn manifest_inputs_disagree_with_report() {
    let b = packed();
    let m = manifest_json(&b);
    for (key, field) in [
        ("task_json", "inputs_sha256.task_json"),
        ("train_jsonl", "inputs_sha256.train_jsonl"),
        ("eval_jsonl", "inputs_sha256.eval_jsonl"),
        ("base_model", "inputs_sha256.base_model"),
        ("tokenizer_json", "inputs_sha256.tokenizer_json"),
    ] {
        let ptr = format!("/inputs_sha256/{key}");
        let flipped = Value::from(flip_hex(
            m.pointer(&ptr).and_then(Value::as_str).expect("sha"),
        ));
        // The report side moves, so the blob-digest bindings still hold.
        let f = forge(&b, &[], &edits(&[(&ptr, flipped)]), &[]);
        assert_field(&f, field);
    }
}

/// `base.sha256` must equal the report's `inputs_sha256.base_model`, even when the recipe
/// (and so `manifest.base`) was re-pinned to agree with the forged value.
#[test]
fn manifest_base_sha_disagrees_with_report() {
    let b = packed();
    let m = manifest_json(&b);
    let flipped = Value::from(flip_hex(m["base"]["sha256"].as_str().expect("sha")));
    let f = forge(
        &b,
        &edits(&[("/base/sha256", flipped.clone())]),
        &[],
        &edits(&[("/base/sha256", flipped)]),
    );
    assert_field(&f, "base.sha256");
}

/// `recipe_id` must equal the embedded report's `recipe_id` (the recipe blob digest is
/// already rung 4's RecipeIdMismatch).
#[test]
fn manifest_recipe_id_disagrees_with_report() {
    let b = packed();
    let m = manifest_json(&b);
    let flipped = Value::from(flip_hex(m["recipe_id"].as_str().expect("recipe_id")));
    let f = forge(&b, &[], &edits(&[("/recipe_id", flipped)]), &[]);
    assert_field(&f, "recipe_id");
}

/// `gate.pass` / `gate.margin` / `gate.ece_post` must equal the embedded report's (f64 bits).
#[test]
fn manifest_gate_summary_disagrees_with_report() {
    let b = packed();
    for (ptr, new, field) in [
        ("/gate/pass", Value::from(true), "gate.pass"),
        ("/gate/margin", Value::from(0.25), "gate.margin"),
        ("/gate/ece_post", Value::from(0.01), "gate.ece_post"),
    ] {
        let f = forge(&b, &[], &[], &edits(&[(ptr, new)]));
        assert_field(&f, field);
    }
}

/// Every calibration field must equal the embedded report's calibration block.
#[test]
fn manifest_calibration_disagrees_with_report() {
    let b = packed();
    for (ptr, report_ptr, new, field) in [
        (
            "/calibration/t_fitted",
            "",
            Value::from(1.5),
            "calibration.t_fitted",
        ),
        (
            "/calibration/clamp_hit",
            "",
            Value::from(true),
            "calibration.clamp_hit",
        ),
        (
            "/calibration/slice_ids_sha256",
            "",
            Value::from("0".repeat(64)),
            "calibration.slice_ids_sha256",
        ),
        // bucket and t_applied are also bound to the task / agent config, so the REPORT
        // moves and the manifest keeps the value the model actually runs with.
        (
            "",
            "/calibration/bucket",
            Value::from("choice:2"),
            "calibration.bucket",
        ),
        (
            "",
            "/calibration/t_applied",
            Value::from(1.5),
            "calibration.t_applied",
        ),
    ] {
        let report = if report_ptr.is_empty() {
            Edits::new()
        } else {
            edits(&[(report_ptr, new.clone())])
        };
        let manifest = if ptr.is_empty() {
            Edits::new()
        } else {
            edits(&[(ptr, new.clone())])
        };
        let f = forge(&b, &[], &report, &manifest);
        assert_field(&f, field);
    }
}

/// `calibration.bucket` must be the task's bucket (`bucket_key(choice, K)`), even when the
/// embedded report agrees with the forged value.
#[test]
fn manifest_calibration_bucket_disagrees_with_task() {
    let b = packed();
    let wrong = edits(&[("/calibration/bucket", Value::from("choice:2"))]);
    let f = forge(&b, &[], &wrong, &wrong);
    assert_field(&f, "calibration.bucket");
}

/// `calibration.t_applied` must be, bit for bit, the temperature the loaded model applies
/// to the task (the agent config's clamped bucket lookup), even when the report agrees.
#[test]
fn manifest_t_applied_disagrees_with_agent() {
    let b = packed();
    // 1.5 is a real temperature of the tiny agent config, for another bucket (choice:2).
    let wrong = edits(&[("/calibration/t_applied", Value::from(1.5))]);
    let f = forge(&b, &[], &wrong, &wrong);
    assert_field(&f, "calibration.t_applied");
    // One ULP away is still a different temperature.
    let m = manifest_json(&b);
    let t = m["calibration"]["t_applied"].as_f64().expect("t_applied");
    let ulp = edits(&[(
        "/calibration/t_applied",
        Value::from(f64::from_bits(t.to_bits() + 1)),
    )]);
    assert_field(&forge(&b, &[], &ulp, &ulp), "calibration.t_applied");
}

/// `device_used` must equal the embedded report's.
#[test]
fn manifest_device_used_disagrees_with_report() {
    let b = packed();
    let f = forge(
        &b,
        &[],
        &[],
        &edits(&[("/device_used", Value::from("mps:0"))]),
    );
    assert_field(&f, "device_used");
}

/// FALSIFY-DECIDE-APR-013: EVERY leaf of the packed manifest, for EVERY binding the
/// contract table names for it, is forged so that exactly that one binding breaks — and
/// the ladder must refuse it at the table's rung. A binding whose check is disabled lets
/// its forgery load, which this test names.
#[test]
fn every_manifest_leaf_is_bound() {
    let b = packed();
    let m = manifest_json(&b);
    let mut ptrs = Vec::new();
    leaves(&m, "", "", &mut ptrs);
    let table = bindings_table();
    let edges = binding_edges(&ptrs, &table);
    let pattern_of: BTreeMap<&str, &str> = ptrs
        .iter()
        .map(|(p, pat)| (p.as_str(), pat.as_str()))
        .collect();
    let mut failures = Vec::new();
    for (i, (leaf, source)) in edges.iter().enumerate() {
        let ptr = leaf.trim_start_matches("manifest:");
        let pattern = pattern_of[ptr];
        let row = &table[pattern];
        let old = m.pointer(ptr).expect("leaf value");
        let forged = break_only(&b, &edges, i, pattern, old);
        match load_verified(&forged) {
            Ok(_) => failures.push(format!(
                "{ptr} <-> {source}: LOADED after the binding was broken"
            )),
            Err(e) => {
                let rung = rung_number(&e);
                if rung != row.rung {
                    failures.push(format!(
                        "{ptr} <-> {source}: refused at rung {rung}, the table names {}: {e}",
                        row.rung
                    ));
                }
                if let ArtifactError::ManifestDisagreesWithBlob { field } = &e {
                    let d = dotted(ptr);
                    if !(d == *field || d.starts_with(&format!("{field}."))) {
                        failures.push(format!(
                            "{ptr} <-> {source}: refused naming another field: {e}"
                        ));
                    }
                }
            }
        }
    }
    println!(
        "every_manifest_leaf_is_bound: {} bindings over {} leaves",
        edges.len(),
        ptrs.len()
    );
    assert!(
        failures.is_empty(),
        "unbound manifest leaves:\n{}",
        failures.join("\n")
    );
}

/// The contract table and the manifest agree leaf for leaf: a stale row, or a manifest leaf
/// added without a binding, fails here naming it. Every source is a known kind and every
/// `recipe:` / `report:` pointer resolves in the embedded blobs.
#[test]
fn manifest_bindings_table_matches_manifest_leaves() {
    let b = packed();
    let mut ptrs = Vec::new();
    leaves(&manifest_json(&b), "", "", &mut ptrs);
    let observed: BTreeSet<String> = ptrs.into_iter().map(|(_, p)| p).collect();
    let table = bindings_table();
    let listed: BTreeSet<String> = table.keys().cloned().collect();
    let unbound: Vec<_> = observed.difference(&listed).collect();
    let stale: Vec<_> = listed.difference(&observed).collect();
    assert!(
        unbound.is_empty(),
        "manifest leaves with no manifest.bindings row: {unbound:?}"
    );
    assert!(
        stale.is_empty(),
        "manifest.bindings rows for no manifest leaf: {stale:?}"
    );

    let r = AprV2ReaderRef::from_bytes(&b).expect("open the packed artifact");
    let json = |name: &str| -> Value {
        serde_json::from_slice(r.get_tensor_data(name).expect("blob")).expect("blob JSON")
    };
    let (recipe, report) = (json(super::RECIPE_BLOB), json(super::GATE_REPORT_BLOB));
    for (leaf, row) in &table {
        assert!([3, 4, 7].contains(&row.rung), "{leaf}: rung {}", row.rung);
        assert!(!row.sources.is_empty(), "{leaf}: no source");
        for s in &row.sources {
            let (kind, rest) = s
                .split_once(':')
                .unwrap_or_else(|| panic!("{leaf}: source {s} has no kind"));
            match kind {
                "recipe" => assert!(
                    recipe.pointer(rest).is_some(),
                    "{leaf}: {s} is not in the recipe blob"
                ),
                "report" => assert!(
                    report.pointer(rest).is_some(),
                    "{leaf}: {s} is not in the gate report"
                ),
                "sha256" => assert!(
                    super::BLOB_TENSORS.contains(&rest),
                    "{leaf}: {s} names no blob"
                ),
                "derived" => assert!(!rest.trim().is_empty(), "{leaf}: {s} says nothing"),
                other => panic!("{leaf}: unknown source kind {other}"),
            }
            assert!(
                !leaf.contains("[*]") || !settable(s),
                "{leaf}: an array leaf may only bind to a fixed source, not {s}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Rung 5: non-finite scan
// ---------------------------------------------------------------------------

/// One F16 weight set to 0x7E00 (a NaN) AFTER packing — weights carry no hash, so only
/// rung 5 can see it.
#[test]
fn nan_weight() {
    let b = edit_tensor(&packed(), "scorer.1.weight", |d| {
        d[..2].copy_from_slice(&0x7E00u16.to_le_bytes());
    });
    let e = refuse(&b);
    assert_eq!(
        e,
        ArtifactError::NonFiniteWeight {
            name: "scorer.1.weight".to_string(),
        }
    );
    assert_eq!(e.rung(), "5 non_finite_scan");
}

// ---------------------------------------------------------------------------
// Rung 7: probe replay
// ---------------------------------------------------------------------------

/// One stored probe probability moved by 2e-5 (over probe_probabilities_abs) is
/// refused at rung 7.
#[test]
fn probe_mismatch() {
    let b = edit_manifest(&packed(), |v| {
        let h = &mut v["probes"][0]["probabilities_f32_hex"][0];
        let bits = u32::from_str_radix(h.as_str().expect("hex"), 16).expect("hex parses");
        let moved = f32::from_bits(bits) + 2e-5;
        *h = serde_json::Value::from(format!("{:08x}", moved.to_bits()));
    });
    let e = refuse(&b);
    assert_eq!(
        e,
        ArtifactError::ProbeMismatch {
            index: 0,
            component: "probabilities",
        }
    );
    assert_eq!(e.rung(), "7 probe_replay");
}

// ---------------------------------------------------------------------------
// Pack-time probe refusals
// ---------------------------------------------------------------------------

/// A probe row over the cap is refused at pack (the cap shrunk to 20 so the 21-token
/// probe 0 exceeds it; the contracted value is asserted in `contract_mirror`).
#[test]
fn probe_row_over_budget() {
    let e = write_decide_apr_within(&inputs(), &ArtifactLimits::tiny(MAX_ARTIFACT_BYTES, 20))
        .expect_err("probe row over the cap");
    assert_eq!(
        e,
        ArtifactError::ProbeRowOverBudget {
            index: 0,
            tokens: 21,
            cap: 20,
        }
    );
}

/// A probes.json expectation shifted by 1e-3 is refused at pack.
#[test]
fn probe_disagrees_with_oracle() {
    let mut i = inputs();
    let h = &mut i.probes[0].probabilities_f32_hex[0];
    let bits = u32::from_str_radix(h, 16).expect("hex parses");
    *h = format!("{:08x}", (f32::from_bits(bits) + 1e-3).to_bits());
    let e = write_decide_apr_within(&i, &ArtifactLimits::CONTRACTED)
        .expect_err("probe disagrees with the oracle");
    assert_eq!(
        e,
        ArtifactError::ProbeDisagreesWithOracle {
            index: 0,
            component: "probabilities",
        }
    );
}
