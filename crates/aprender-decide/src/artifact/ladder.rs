//! FALSIFY-DECIDE-APR-001/-008/-009/-011: every ladder rung refuses its own induced
//! negative, naming the rung. Negatives are induced by editing the PACKED bytes (or by
//! packing a mutated in-memory [`PackInputs`]) — never by weakening a rung.

use super::tests::pack_tiny;
use super::{
    check_index_extent, check_size, expected_bytes, load_verified, load_verified_within,
    read_bounded_within, read_decide_apr_bytes_bounded, write_decide_apr_within, ArtifactError,
    ArtifactLimits, CUSTOM_METADATA_KEY, MAX_ARTIFACT_BYTES, MAX_TENSOR_COUNT,
    MIN_INDEX_ENTRY_BYTES, PROBE_MAX_ROW_TOKENS,
};
use crate::pack::PackInputs;
use crate::test_support::fixture_dir;
use aprender::format::v2::{
    AprV2Header, AprV2Metadata, AprV2ReaderRef, AprV2Writer, TensorDType, HEADER_SIZE_V2,
};
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
