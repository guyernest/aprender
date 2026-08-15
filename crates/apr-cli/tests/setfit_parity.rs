//! The three-surface SetFit parity gate (Phase 4 D-13 / D-14).
//!
//! # What this file proves, and what it deliberately does not
//!
//! It feeds ONE ordered input set through THREE surfaces and compares them
//! pairwise:
//!
//! | leg | reached by |
//! | --- | ---------- |
//! | library | `aprender::setfit::VerifiedSetFitModel::classify` |
//! | CLI | a SPAWNED `env!("CARGO_BIN_EXE_apr")` running `predict --input … --json` |
//! | HTTP | `realizar::api::create_router_with_config` driven by `tower` `oneshot` |
//!
//! **It does NOT prove that training produces an artifact all three agree on.**
//! That chain cannot close on this host: `CALIBRATED_REGIMES` admits exactly one
//! encoder (the phase-3 MiniLM slice) whose 97-row vocabulary closure cannot
//! compute the `probe_unicode` probe, so no `setfit-apr-v1` artifact is
//! producible from `apr setfit train` here (finding F-10, measured on three
//! independent routes by plan 04-12; it is a Phase 5 item). The artifact this
//! file compares over is a SYNTHETIC fixture written through core's public
//! `write_setfit_apr` and loaded through core's public `load_setfit_apr` — the
//! same two doors production uses. That is honest for a PARITY claim, which is
//! about three READERS agreeing on one artifact, and it would NOT be honest for
//! an end-to-end claim, which this file does not make.
//!
//! # Why the input set is one document and not one text per line (review M2)
//!
//! The probe set contains `probe_whitespace`, a text with an embedded newline
//! and a tab. A line-delimited CLI input format would have re-split that text
//! into two CLI texts while the HTTP leg still posted one — so the three legs
//! would have received DIFFERENT ordered input sets and the gate would have
//! reported agreement it had never tested. All three legs here carry the
//! identical `ClassifyRequestDocument`, and the CLI leg is handed the SAME
//! serialized bytes the HTTP leg posts. `parity_every_leg_returns_one_result_per_text`
//! is the standing witness for that.
//!
//! # Latency is compared for neither equality nor positivity
//!
//! `latency_ms` is a MEASUREMENT. Core's `PartialEq for ClassifyResponse`
//! already excludes it, and this comparator checks only `is_finite() && >= 0.0`:
//! a fast operation under a coarse timer legitimately reports `0.0`, so a
//! `> 0.0` assertion is a flake with a schedule.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use aprender::setfit::artifact::{PROBE_TRUNCATION_REPEAT_COUNT, PROBE_TRUNCATION_REPEAT_UNIT};
use aprender::setfit::{
    artifact_sha256_hex, load_setfit_apr, write_setfit_apr, ClassifyRequestDocument,
    ClassifyResponse, EncoderArchitecture, SetFitArtifactView, VerifiedSetFitModel,
    PROBE_LOGITS_ABS_TOLERANCE, PROBE_PROBABILITIES_ABS_TOLERANCE,
};

// ---------------------------------------------------------------------------
// The tiny fixture artifact
// ---------------------------------------------------------------------------

mod fixture {
    //! A `setfit-apr-v1` artifact small enough to build at test start.
    //!
    //! **A DUPLICATE of `aprender-core`'s `setfit::artifact::fixture` (the
    //! `fixture_view_full_pin_shape()` recipe), by way of `aprender-serve`'s own
    //! copy in `api/setfit_handlers.rs`.** Core's is `#[cfg(test)]` and plan
    //! 03-10's acceptance criteria reject a `#[doc(hidden)]` test-support door on
    //! the shipped surface, so it is unreachable from an integration test by
    //! design rather than by oversight.
    //!
    //! What is duplicated is the SHAPE, and every part is built through core's
    //! PUBLIC API: `SetFitArtifactView` and `EncoderArchitecture` have public
    //! fields, `write_setfit_apr` / `load_setfit_apr` are the two production
    //! doors, and `artifact_sha256_hex` is the crate's one hashing path. If the
    //! artifact schema changes this builder stops producing a loadable artifact
    //! and these tests go red, which is the correct coupling.
    //!
    //! It is the FULL PRODUCTION NULLABILITY SHAPE — `vocab_remap: None`,
    //! `evidence.epsilon_used: null` — so the parity legs run on the shape a
    //! pinned MiniLM serializes, not a nullability-free simplification.
    //!
    //! The ~90 MB real model is NOT used and nothing large is committed: the
    //! whole artifact is a few hundred kilobytes and lives in a tempdir.

    use super::{
        artifact_sha256_hex, load_setfit_apr, write_setfit_apr, BTreeMap, EncoderArchitecture,
        SetFitArtifactView, VerifiedSetFitModel,
    };
    use aprender::setfit::{
        L2_EPS, MAX_SEQUENCE_LENGTH, NORMALIZATION_POLICY, PADDING_MODE, PINNED_ACTIVATION,
        PINNED_REVISION, POOLING_POLICY,
    };
    use serde_json::json;

    const FIXTURE_HIDDEN: usize = 8;
    const FIXTURE_HEADS: usize = 2;
    const FIXTURE_LAYERS: usize = 2;
    const FIXTURE_INTERMEDIATE: usize = 16;
    const FIXTURE_TYPE_VOCAB: usize = 2;
    pub const FIXTURE_LABELS: [&str; 3] = ["against", "favor", "neutral"];

    /// A tiny WordPiece vocabulary. Everything outside it becomes `[UNK]`, so
    /// every id the tokenizer can emit is `< TINY_VOCAB.len()` — which is what
    /// lets the fixture carry `vocab_remap: None` (the PRODUCTION shape) with a
    /// 48-row embedding table instead of the pin's 30522.
    const TINY_VOCAB: [&str; 48] = [
        "[PAD]",
        "[UNK]",
        "[CLS]",
        "[SEP]",
        "[MASK]",
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "lazy",
        "dog",
        "ok",
        "few",
        "shot",
        "classification",
        "with",
        "contrastive",
        "pairs",
        "line",
        "one",
        "two",
        "tabbed",
        "spaced",
        "stance",
        "detection",
        "i",
        "firmly",
        "support",
        "this",
        "position",
        "el",
        "zorro",
        "cafe",
        "naive",
        "pi",
        "##s",
        "##ed",
        ".",
        ",",
        "!",
        "#",
        "@",
        ":",
        "/",
        "-",
        "=",
    ];

    /// A valid, self-contained `tokenizer.json` in the pinned file's exact shape
    /// (BertNormalizer + BertPreTokenizer + TemplateProcessing + WordPiece) with
    /// [`TINY_VOCAB`] substituted for the 30522-entry pin.
    ///
    /// Self-contained ON PURPOSE: reading the committed fixture file would honour
    /// the `APRENDER_SETFIT_FIXTURES` override, and a test whose input depends on
    /// an environment variable is not a test.
    fn tiny_tokenizer_json() -> Vec<u8> {
        use std::fmt::Write as _;
        let mut s = String::new();
        s.push_str(r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":["#);
        for (id, content) in ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]"]
            .iter()
            .enumerate()
        {
            if id > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                r#"{{"id":{id},"special":true,"content":"{content}","single_word":false,"lstrip":false,"rstrip":false,"normalized":false}}"#
            );
        }
        s.push_str(
            r###"],"normalizer":{"type":"BertNormalizer","clean_text":true,"handle_chinese_chars":true,"strip_accents":null,"lowercase":true},"pre_tokenizer":{"type":"BertPreTokenizer"},"post_processor":{"type":"TemplateProcessing","single":[{"SpecialToken":{"id":"[CLS]","type_id":0}},{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"[SEP]","type_id":0}}],"pair":[{"SpecialToken":{"id":"[CLS]","type_id":0}},{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"[SEP]","type_id":0}},{"Sequence":{"id":"B","type_id":1}},{"SpecialToken":{"id":"[SEP]","type_id":1}}],"special_tokens":{"[CLS]":{"id":"[CLS]","ids":[2],"tokens":["[CLS]"]},"[SEP]":{"id":"[SEP]","ids":[3],"tokens":["[SEP]"]}}},"decoder":{"type":"WordPiece","prefix":"##","cleanup":true},"model":{"type":"WordPiece","unk_token":"[UNK]","continuing_subword_prefix":"##","max_input_chars_per_word":100,"vocab":{"###,
        );
        for (id, token) in TINY_VOCAB.iter().enumerate() {
            if id > 0 {
                s.push(',');
            }
            let _ = write!(s, r#""{token}":{id}"#);
        }
        s.push_str("}}}");
        s.into_bytes()
    }

    /// A deterministic, platform-independent filler.
    ///
    /// Every produced value is `k / 65536 - 0.5` for an integer `k`, so it is
    /// EXACTLY representable in `f32` on every target: the fixture's own bytes
    /// cannot be a source of cross-platform drift, and neither can the goldens
    /// frozen from it.
    struct Filler(u64);

    impl Filler {
        fn new(seed: u64) -> Self {
            Self(seed | 1)
        }

        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let quantum = f32::from(u16::try_from((self.0 >> 40) & 0xFFFF).unwrap_or(0)) / 65536.0;
            quantum - 0.5
        }

        fn vec(&mut self, n: usize) -> Vec<f32> {
            (0..n).map(|_| self.next()).collect()
        }
    }

    fn fixture_architecture() -> EncoderArchitecture {
        EncoderArchitecture {
            hidden: FIXTURE_HIDDEN,
            heads: FIXTURE_HEADS,
            head_dim: FIXTURE_HIDDEN / FIXTURE_HEADS,
            num_layers: FIXTURE_LAYERS,
            intermediate: FIXTURE_INTERMEDIATE,
            vocab: TINY_VOCAB.len(),
            positions: MAX_SEQUENCE_LENGTH,
            type_vocab_size: FIXTURE_TYPE_VOCAB,
            layer_norm_eps: 1e-12,
            pad_token_id: 0,
            hidden_act: PINNED_ACTIVATION.to_string(),
            source_revision: PINNED_REVISION.to_string(),
            tokenizer_sha256: artifact_sha256_hex(&tiny_tokenizer_json()),
            // `None` is the PRODUCTION shape (a pinned MiniLM serializes no
            // remap), and it is what makes the writer's nullability walk
            // non-vacuous on this fixture.
            vocab_remap: None,
        }
    }

    fn put(
        t: &mut BTreeMap<String, (Vec<usize>, Vec<f32>)>,
        f: &mut Filler,
        name: String,
        shape: Vec<usize>,
    ) {
        let n = shape.iter().product();
        t.insert(name, (shape, f.vec(n)));
    }

    fn fixture_tensors(arch: &EncoderArchitecture) -> BTreeMap<String, (Vec<usize>, Vec<f32>)> {
        let h = arch.hidden;
        let im = arch.intermediate;
        let mut f = Filler::new(0x0409_0001);
        let mut t: BTreeMap<String, (Vec<usize>, Vec<f32>)> = BTreeMap::new();

        put(
            &mut t,
            &mut f,
            "embeddings.word_embeddings.weight".to_string(),
            vec![arch.vocab, h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.position_embeddings.weight".to_string(),
            vec![arch.positions, h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.token_type_embeddings.weight".to_string(),
            vec![arch.type_vocab_size, h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.LayerNorm.weight".to_string(),
            vec![h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.LayerNorm.bias".to_string(),
            vec![h],
        );

        for n in 0..arch.num_layers {
            let p = format!("encoder.layer.{n}");
            for leaf in ["query", "key", "value"] {
                put(
                    &mut t,
                    &mut f,
                    format!("{p}.attention.self.{leaf}.weight"),
                    vec![h, h],
                );
                put(
                    &mut t,
                    &mut f,
                    format!("{p}.attention.self.{leaf}.bias"),
                    vec![h],
                );
            }
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.dense.weight"),
                vec![h, h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.dense.bias"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.LayerNorm.weight"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.LayerNorm.bias"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.intermediate.dense.weight"),
                vec![im, h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.intermediate.dense.bias"),
                vec![im],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.output.dense.weight"),
                vec![h, im],
            );
            put(&mut t, &mut f, format!("{p}.output.dense.bias"), vec![h]);
            put(
                &mut t,
                &mut f,
                format!("{p}.output.LayerNorm.weight"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.output.LayerNorm.bias"),
                vec![h],
            );
        }
        t
    }

    /// The full-pin-shape view — the `fixture_view_full_pin_shape()` recipe.
    ///
    /// `disallowed_methods` is allowed for the `serde_json::json!` expansions
    /// below and for NOTHING ELSE: the macro's own generated code calls
    /// `Result::unwrap` on an infallible `Value` construction. There is no
    /// `unwrap` written in this file (the source assertion
    /// `parity_harness_writes_no_unwrap` pins that), so the allow cannot be
    /// silently widened into hand-written fallibility.
    #[allow(clippy::disallowed_methods)]
    fn fixture_view_full_pin_shape() -> SetFitArtifactView {
        let architecture = fixture_architecture();
        let tensors = fixture_tensors(&architecture);
        let mut head = Filler::new(0x0409_0002);
        let k = FIXTURE_LABELS.len();
        let max_len = u32::try_from(MAX_SEQUENCE_LENGTH).expect("256 fits in u32");
        SetFitArtifactView {
            bundle_schema_version: 1,
            format_id: "setfit-apr-v1-fixture".to_string(),
            tokenizer_bytes: tiny_tokenizer_json(),
            head_weights: head.vec(k * architecture.hidden),
            head_intercepts: head.vec(k),
            head_n_features: architecture.hidden,
            architecture,
            tensors,
            pooling: POOLING_POLICY.to_string(),
            normalization: NORMALIZATION_POLICY.to_string(),
            l2_epsilon: L2_EPS,
            truncation_max_sequence_length: max_len,
            padding_mode: PADDING_MODE.to_string(),
            max_length: max_len,
            root_seed: 0x0409_0000_0000_0002,
            ordered_labels: FIXTURE_LABELS.iter().map(|s| (*s).to_string()).collect(),
            requested_config: json!({
                "max_length": 256,
                "pair_config": { "budget": null, "hard_cap": null, "strategy": "all_pairs" },
                "requested_device": "cpu",
                "seed": 7
            }),
            resolved_config: json!({ "resolved_device": "cpu" }),
            evidence: json!({
                "epsilon_used": null,
                "table_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "per_class": {
                    "against": { "support": 8, "mean_margin": 0.1 },
                    "favor": { "support": 8, "mean_margin": 0.5 },
                    "neutral": { "support": 8, "mean_margin": 0.25 }
                }
            }),
            provenance: json!({
                "dataset_fingerprint": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                "validation_split_fingerprint": "vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv",
                "selection_semantic_hash": "ssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss",
                "selection_ledger_hash": "llllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllll",
                "selection_root_seed": 11,
                "shots_per_class": 8
            }),
        }
    }

    /// The artifact BYTES.
    pub fn fixture_bytes() -> Vec<u8> {
        write_setfit_apr(&fixture_view_full_pin_shape()).expect("the tiny fixture is writable")
    }

    /// Load a model from bytes through the ONE production door.
    ///
    /// `load_setfit_apr` runs the whole ladder including probe replay, so this
    /// helper cannot mint a `VerifiedSetFitModel` that skipped a rung — and the
    /// fact that it succeeds is itself evidence the duplicated shape is still a
    /// valid artifact.
    pub fn load(bytes: &[u8]) -> VerifiedSetFitModel {
        load_setfit_apr(bytes).expect("the tiny fixture passes every load rung")
    }
}

// ---------------------------------------------------------------------------
// The ONE committed input set
// ---------------------------------------------------------------------------

/// All six contract-resident probe strings plus two extras.
///
/// The extras are the EMPTY string (an entry a `filter(|s| !s.is_empty())`
/// anywhere in a surface would silently drop, shifting every later index) and a
/// 4-byte-UTF-8 emoji run (the pangram and unicode probes only reach 3-byte
/// sequences). `probe_whitespace` carries both an embedded newline and a tab —
/// the review-M2 witness.
fn parity_texts() -> Vec<String> {
    vec![
        // probe_minimal
        "ok".to_string(),
        // probe_ascii_pangram
        "the quick brown fox jumps over the lazy dog".to_string(),
        // probe_unicode
        "El rapido zorro marron salta sobre el perro perezoso — naive cafe, pi = 3.14159"
            .to_string(),
        // probe_truncation_boundary, built from the contract's own constants
        PROBE_TRUNCATION_REPEAT_UNIT.repeat(PROBE_TRUNCATION_REPEAT_COUNT),
        // probe_social
        "Stance detection: I firmly support this position!!! #debate @user123 https://example.com"
            .to_string(),
        // probe_whitespace — an embedded newline AND a tab AND a run of spaces
        "line one\nline two\ttabbed   spaced".to_string(),
        // extra: the empty string
        String::new(),
        // extra: 4-byte UTF-8
        "🦊🌮 café naïve π ✅".to_string(),
    ]
}

/// The batch document every pairwise test runs on.
fn batch_document() -> ClassifyRequestDocument {
    ClassifyRequestDocument::new(parity_texts())
}

/// A single-text document — the arity a batch cannot witness.
fn single_document() -> ClassifyRequestDocument {
    ClassifyRequestDocument::new(["line one\nline two\ttabbed   spaced"])
}

/// The batch document with per-class logits requested.
fn logits_document() -> ClassifyRequestDocument {
    batch_document().with_logits()
}

// ---------------------------------------------------------------------------
// The harness: one artifact, one serialized document, three legs
// ---------------------------------------------------------------------------

/// One fixture artifact on disk plus the model the LIBRARY leg reads through.
struct Harness {
    _dir: tempfile::TempDir,
    apr_path: std::path::PathBuf,
    bytes: Vec<u8>,
    model: VerifiedSetFitModel,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a tempdir is creatable");
        let apr_path = dir.path().join("parity-fixture.apr");
        let bytes = fixture::fixture_bytes();
        std::fs::write(&apr_path, &bytes).expect("the fixture artifact is writable");
        let model = fixture::load(&bytes);
        Self {
            _dir: dir,
            apr_path,
            bytes,
            model,
        }
    }

    /// The hash of the bytes on disk, derived independently of any surface.
    fn artifact_sha256(&self) -> String {
        artifact_sha256_hex(&self.bytes)
    }

    /// Serialize the document ONCE. These exact bytes are what the CLI leg reads
    /// from a file and what the HTTP leg posts as a body — so no leg can be
    /// handed a differently-shaped input than another (review M2).
    fn serialize(document: &ClassifyRequestDocument) -> String {
        serde_json::to_string(document).expect("the request document serializes")
    }

    fn write_document(&self, document: &ClassifyRequestDocument) -> std::path::PathBuf {
        let path = self._dir.path().join(format!(
            "request-{}-{}.json",
            document.texts.len(),
            usize::from(document.include_logits)
        ));
        std::fs::write(&path, Self::serialize(document)).expect("the request document is writable");
        path
    }

    // -- leg (a): the library --------------------------------------------------

    fn library(&self, document: &ClassifyRequestDocument) -> ClassifyResponse {
        self.model
            .classify(document)
            .expect("the library leg classifies the committed document")
    }

    // -- leg (b): the spawned CLI ---------------------------------------------

    /// Spawn `apr predict <apr> --input <json> --json` and parse stdout INTO
    /// core's envelope.
    ///
    /// The binary is resolved by `CARGO_BIN_EXE_apr` and NEVER by a PATH lookup:
    /// four `apr` binaries have coexisted on this dev box and a bare `apr` once
    /// resolved to a 26-day-old copy (CLAUDE.md discipline 3 and 8, T-04-29).
    ///
    /// The status is read from `output.status.success()` — never through a pipe
    /// (CLAUDE.md Verification rule 1).
    ///
    /// Parsing INTO `ClassifyResponse` rather than into a `serde_json::Value` is
    /// deliberate: any CLI re-keying, dropped field or malformed row becomes a
    /// deserialization failure here, and core's validated `Deserialize` also
    /// proves the CLI emitted a well-formed envelope rather than merely valid
    /// JSON.
    fn cli(&self, document: &ClassifyRequestDocument) -> ClassifyResponse {
        let doc_path = self.write_document(document);
        cli_predict(&self.apr_path, &doc_path)
    }

    // -- leg (c): in-process HTTP ---------------------------------------------

    /// POST the SAME serialized document to the REAL router.
    ///
    /// The model in the router's slot is an INDEPENDENT `load_setfit_apr` of the
    /// same bytes, so the artifact-hash equality this gate asserts is a claim
    /// about two loads rather than about one shared object.
    fn http(&self, document: &ClassifyRequestDocument) -> ClassifyResponse {
        let body = Self::serialize(document);
        let model = fixture::load(&self.bytes);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a current-thread runtime is buildable");
        runtime.block_on(async move {
            use realizar::api::{create_router_with_config, AppState, RouterConfig};
            use tower::util::ServiceExt;

            let state = AppState::default().with_setfit_model(std::sync::Arc::new(model));
            let app = create_router_with_config(state, RouterConfig::default());
            let response = app
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/v1/classify")
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(body))
                        .expect("the request is well formed"),
                )
                .await
                .expect("the router answers");
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("the body is readable");
            assert_eq!(
                status,
                axum::http::StatusCode::OK,
                "the HTTP leg must answer 200; body = {}",
                String::from_utf8_lossy(&bytes)
            );
            serde_json::from_slice::<ClassifyResponse>(&bytes)
                .expect("the HTTP body deserializes INTO core's envelope")
        })
    }
}

/// The spawned-CLI call, factored out so the smoke test and the parity legs
/// resolve the binary through exactly one place.
fn cli_predict(apr_path: &Path, doc_path: &Path) -> ClassifyResponse {
    let output = Command::new(env!("CARGO_BIN_EXE_apr"))
        .arg("predict")
        .arg(apr_path)
        .arg("--input")
        .arg(doc_path)
        .arg("--json")
        .output()
        .expect("the apr binary is spawnable");
    assert!(
        output.status.success(),
        "apr predict failed: status={:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<ClassifyResponse>(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "apr predict --json did not emit core's envelope ({error}); stdout was:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

// ---------------------------------------------------------------------------
// The comparator — the thing this whole plan is a gate around
// ---------------------------------------------------------------------------

/// The NaN-VISIBLE comparison the contract mandates.
///
/// `matches!(delta.partial_cmp(&bound), Some(Less | Equal))`. A bare
/// `delta <= bound` is a contract violation: it happens to reject NaN in this
/// direction, but refactors into `!(delta > bound)`, which ACCEPTS NaN silently.
fn within(delta: f64, bound: f64) -> bool {
    matches!(
        delta.partial_cmp(&bound),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    )
}

/// Every way two surfaces can disagree, named.
///
/// A RETURNED value rather than a panic, so the in-band negative can assert
/// rejection by `matches!` instead of by log inspection.
#[derive(Debug, Clone, PartialEq)]
enum ParityMismatch {
    SchemaVersion {
        left: u32,
        right: u32,
    },
    ArtifactSha256 {
        left: String,
        right: String,
    },
    Backend {
        left: String,
        right: String,
    },
    ResultCount {
        left: usize,
        right: usize,
    },
    Label {
        index: usize,
        left: String,
        right: String,
    },
    ProbabilityArity {
        index: usize,
        left: usize,
        right: usize,
    },
    Probability {
        index: usize,
        class: usize,
        left: f64,
        right: f64,
    },
    LogitsPresence {
        index: usize,
        left: bool,
        right: bool,
    },
    Logit {
        index: usize,
        class: usize,
        left: f64,
        right: f64,
    },
    Margin {
        index: usize,
        left: f64,
        right: f64,
    },
    TokenCount {
        index: usize,
        left: u32,
        right: u32,
    },
    Truncated {
        index: usize,
        left: bool,
        right: bool,
    },
    /// `latency_ms` is checked for FINITENESS and non-negativity only — never for
    /// equality and never for strict positivity.
    LatencyNotAMeasurement {
        side: &'static str,
        value: f64,
    },
}

/// Compare two envelopes at the contract's parity tolerances.
///
/// Exact: labels, `artifact_sha256`, `schema_version`, `backend`, `token_count`,
/// `truncated`, result arity, logits presence.
/// Tolerance `1.0e-5` (`parity_probabilities_abs` / `parity_logits_abs`, the same
/// numbers core exports as `PROBE_*_ABS_TOLERANCE` — read from ONE place):
/// probabilities, logits, margins.
/// Neither: `latency_ms`.
#[allow(clippy::too_many_lines)]
fn compare_parity(left: &ClassifyResponse, right: &ClassifyResponse) -> Result<(), ParityMismatch> {
    for (side, response) in [("left", left), ("right", right)] {
        let latency = response.latency_ms();
        if !latency.is_finite() || latency < 0.0 {
            return Err(ParityMismatch::LatencyNotAMeasurement {
                side,
                value: latency,
            });
        }
    }

    if left.schema_version() != right.schema_version() {
        return Err(ParityMismatch::SchemaVersion {
            left: left.schema_version(),
            right: right.schema_version(),
        });
    }
    if left.artifact_sha256() != right.artifact_sha256() {
        return Err(ParityMismatch::ArtifactSha256 {
            left: left.artifact_sha256().to_string(),
            right: right.artifact_sha256().to_string(),
        });
    }
    if left.backend() != right.backend() {
        return Err(ParityMismatch::Backend {
            left: left.backend().to_string(),
            right: right.backend().to_string(),
        });
    }
    if left.results().len() != right.results().len() {
        return Err(ParityMismatch::ResultCount {
            left: left.results().len(),
            right: right.results().len(),
        });
    }

    for (index, (a, b)) in left
        .results()
        .iter()
        .zip(right.results().iter())
        .enumerate()
    {
        if a.label() != b.label() {
            return Err(ParityMismatch::Label {
                index,
                left: a.label().to_string(),
                right: b.label().to_string(),
            });
        }
        if a.probabilities().len() != b.probabilities().len() {
            return Err(ParityMismatch::ProbabilityArity {
                index,
                left: a.probabilities().len(),
                right: b.probabilities().len(),
            });
        }
        for (class, (p, q)) in a
            .probabilities()
            .iter()
            .zip(b.probabilities().iter())
            .enumerate()
        {
            if !within((p - q).abs(), PROBE_PROBABILITIES_ABS_TOLERANCE) {
                return Err(ParityMismatch::Probability {
                    index,
                    class,
                    left: *p,
                    right: *q,
                });
            }
        }
        match (a.logits(), b.logits()) {
            (Some(x), Some(y)) => {
                if x.len() != y.len() {
                    return Err(ParityMismatch::ProbabilityArity {
                        index,
                        left: x.len(),
                        right: y.len(),
                    });
                }
                for (class, (p, q)) in x.iter().zip(y.iter()).enumerate() {
                    if !within((p - q).abs(), PROBE_LOGITS_ABS_TOLERANCE) {
                        return Err(ParityMismatch::Logit {
                            index,
                            class,
                            left: *p,
                            right: *q,
                        });
                    }
                }
            }
            (None, None) => {}
            (x, y) => {
                return Err(ParityMismatch::LogitsPresence {
                    index,
                    left: x.is_some(),
                    right: y.is_some(),
                })
            }
        }
        if !within(
            (a.margin() - b.margin()).abs(),
            PROBE_PROBABILITIES_ABS_TOLERANCE,
        ) {
            return Err(ParityMismatch::Margin {
                index,
                left: a.margin(),
                right: b.margin(),
            });
        }
        if a.token_count() != b.token_count() {
            return Err(ParityMismatch::TokenCount {
                index,
                left: a.token_count(),
                right: b.token_count(),
            });
        }
        if a.truncated() != b.truncated() {
            return Err(ParityMismatch::Truncated {
                index,
                left: a.truncated(),
                right: b.truncated(),
            });
        }
    }
    Ok(())
}

fn assert_parity(what: &str, left: &ClassifyResponse, right: &ClassifyResponse) {
    if let Err(mismatch) = compare_parity(left, right) {
        panic!("{what}: the surfaces disagree — {mismatch:?}");
    }
}

/// `backend` is an execution-derived IDENTITY (D-12), so it is compared for
/// equality across legs and asserted to carry no CPU-capability token: an
/// availability detection describes the host, never the run that happened.
fn assert_backend_is_a_capability_free_identity(response: &ClassifyResponse) {
    let backend = response.backend();
    assert_eq!(
        backend.split(':').count(),
        3,
        "the backend identity's grammar is three colon-separated segments; got {backend:?}"
    );
    let lowered = backend.to_ascii_lowercase();
    for forbidden in ["avx", "sse", "neon", "simd", "fma"] {
        assert!(
            !lowered.contains(forbidden),
            "backend {backend:?} names a CPU capability ({forbidden}); a capability describes the \
             HOST, not the dispatch that ran (D-12, review B6)"
        );
    }
}

// ---------------------------------------------------------------------------
// The pairwise tests
// ---------------------------------------------------------------------------

#[test]
fn parity_library_and_cli_agree_on_the_batch_document() {
    let harness = Harness::new();
    let document = batch_document();
    let library = harness.library(&document);
    let cli = harness.cli(&document);
    assert_eq!(
        library.artifact_sha256(),
        harness.artifact_sha256(),
        "the library leg reports the hash of the bytes on disk"
    );
    assert_eq!(
        cli.artifact_sha256(),
        harness.artifact_sha256(),
        "the CLI leg reports the hash of the bytes on disk"
    );
    assert_backend_is_a_capability_free_identity(&library);
    assert_parity("library vs CLI (batch)", &library, &cli);
}

#[test]
fn parity_library_and_http_agree_on_the_batch_document() {
    let harness = Harness::new();
    let document = batch_document();
    let library = harness.library(&document);
    let http = harness.http(&document);
    assert_eq!(http.artifact_sha256(), harness.artifact_sha256());
    assert_backend_is_a_capability_free_identity(&http);
    assert_parity("library vs HTTP (batch)", &library, &http);
}

#[test]
fn parity_cli_and_http_agree_on_the_batch_document() {
    let harness = Harness::new();
    let document = batch_document();
    let cli = harness.cli(&document);
    let http = harness.http(&document);
    assert_eq!(cli.artifact_sha256(), harness.artifact_sha256());
    assert_eq!(http.artifact_sha256(), harness.artifact_sha256());
    assert_parity("CLI vs HTTP (batch)", &cli, &http);
    // Core's own `PartialEq` excludes `latency_ms` (F-14(3)), so the plain
    // comparison is the intended one and is asserted alongside the comparator.
    assert_eq!(cli, http, "core's envelope equality holds across surfaces");
}

#[test]
fn parity_all_three_agree_on_a_single_text_document() {
    let harness = Harness::new();
    let document = single_document();
    let library = harness.library(&document);
    let cli = harness.cli(&document);
    let http = harness.http(&document);
    assert_parity("single: library vs CLI", &library, &cli);
    assert_parity("single: library vs HTTP", &library, &http);
    assert_parity("single: CLI vs HTTP", &cli, &http);
    for (leg, response) in [("library", &library), ("cli", &cli), ("http", &http)] {
        assert_eq!(
            response.results().len(),
            1,
            "{leg} returned {} results for a one-text document",
            response.results().len()
        );
    }
}

#[test]
fn parity_all_three_agree_with_logits_requested() {
    let harness = Harness::new();
    let document = logits_document();
    let library = harness.library(&document);
    let cli = harness.cli(&document);
    let http = harness.http(&document);
    for (leg, response) in [("library", &library), ("cli", &cli), ("http", &http)] {
        for (index, result) in response.results().iter().enumerate() {
            let logits = result
                .logits()
                .unwrap_or_else(|| panic!("{leg} result {index} carries no logits"));
            assert_eq!(
                logits.len(),
                result.probabilities().len(),
                "{leg} result {index}: logit arity must equal probability arity"
            );
        }
    }
    assert_parity("logits: library vs CLI", &library, &cli);
    assert_parity("logits: library vs HTTP", &library, &http);
    assert_parity("logits: CLI vs HTTP", &cli, &http);
}

/// The input-fidelity witness (review M2, T-04-53).
///
/// A line-delimited CLI format would have re-split `probe_whitespace` into two
/// texts, so the CLI leg would have returned MORE results than the document had
/// texts while every value it did return still matched. This is the assertion
/// that would have caught it.
#[test]
fn parity_every_leg_returns_one_result_per_text() {
    let harness = Harness::new();
    let document = batch_document();
    let expected = document.texts.len();
    assert!(
        document.texts.iter().any(|t| t.contains('\n')),
        "the committed input set must contain a newline-bearing text or this test is vacuous"
    );
    assert!(
        document.texts.iter().any(|t| t.contains('\t')),
        "the committed input set must contain a tab-bearing text or this test is vacuous"
    );
    assert!(
        document.texts.iter().any(String::is_empty),
        "the committed input set must contain the empty string or this test is vacuous"
    );
    assert!(
        document.texts.iter().any(|t| !t.is_ascii()),
        "the committed input set must contain non-ASCII or this test is vacuous"
    );

    for (leg, response) in [
        ("library", harness.library(&document)),
        ("cli", harness.cli(&document)),
        ("http", harness.http(&document)),
    ] {
        assert_eq!(
            response.results().len(),
            expected,
            "{leg} returned {} results for a {expected}-text document — a surface that \
             re-split or dropped a text",
            response.results().len()
        );
    }
}

/// The truncation-boundary probe must actually truncate, on every leg.
///
/// Without this, `truncated`/`token_count` parity could hold vacuously by both
/// legs reporting `false`/small on every text.
#[test]
fn parity_the_truncation_boundary_text_truncates_on_every_leg() {
    let harness = Harness::new();
    let document = batch_document();
    let boundary = document
        .texts
        .iter()
        .position(|t| t.len() > 1000)
        .expect("the truncation-boundary probe is in the committed input set");
    for (leg, response) in [
        ("library", harness.library(&document)),
        ("cli", harness.cli(&document)),
        ("http", harness.http(&document)),
    ] {
        let result = &response.results()[boundary];
        assert!(
            result.truncated(),
            "{leg}: the truncation-boundary probe must report truncated == true"
        );
        assert!(
            result.token_count() > 0,
            "{leg}: a truncated text consumed a positive number of positions"
        );
    }
}

// ---------------------------------------------------------------------------
// Source assertions over this file
// ---------------------------------------------------------------------------

/// This file's own source.
fn harness_source() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/setfit_parity.rs"
    ))
    .expect("the harness can read its own source")
}

/// CODE lines only.
///
/// A guard that scans documentation fails on its own explanation of the rule it
/// enforces — orchestrator finding F-05, observed twice in plan 04-07. The
/// module header above deliberately spells `latency_ms` and describes the
/// line-delimited format this gate forbids; neither is behaviour.
///
/// Deliberately implemented with `split('\n')` rather than the obvious
/// iterator, so the forbidden-needle scan below can be honest about the fact
/// that the harness never splits text into lines.
fn code_lines(source: &str) -> Vec<&str> {
    source
        .split('\n')
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .collect()
}

/// A needle assembled at RUNTIME from fragments.
///
/// A literal needle would appear in the file being scanned — this file — so
/// every `contains` would be satisfied by its own source and the measurement
/// would be zero-information. Assembling means a hit is a real hit.
fn needle(parts: &[&str]) -> String {
    parts.concat()
}

fn count_occurrences(haystack: &[&str], needle: &str) -> usize {
    haystack
        .iter()
        .map(|line| line.matches(needle).count())
        .sum()
}

#[test]
fn parity_harness_resolves_the_binary_through_cargo_and_never_through_path() {
    let source = harness_source();
    let code = code_lines(&source);
    assert!(
        code.len() > 400,
        "the code-line filter ate the file ({} lines survived) — a vacuous scan",
        code.len()
    );

    let cargo_bin = needle(&["CARGO_BIN_", "EXE_apr"]);
    assert!(
        count_occurrences(&code, &cargo_bin) >= 1,
        "the binary must be resolved through cargo's own env var"
    );

    // A PATH lookup is the shadowed-artifact defect (CLAUDE.md discipline 3/8,
    // T-04-29): four `apr` binaries have coexisted on this box.
    for forbidden in [
        needle(&["Command::new(\"a", "pr\")"]),
        needle(&["Command::new(\"a", "pr\".to_string())"]),
    ] {
        assert_eq!(
            count_occurrences(&code, &forbidden),
            0,
            "the harness must never resolve `apr` through PATH ({forbidden})"
        );
    }
}

#[test]
fn parity_harness_never_splits_an_input_text_into_lines() {
    let source = harness_source();
    let code = code_lines(&source);

    // Review M2: a line-delimited CLI format would have re-split
    // `probe_whitespace`, so the three legs would have compared different
    // ordered input sets while reporting agreement.
    let lines_call = needle(&[".li", "nes()"]);
    assert_eq!(
        count_occurrences(&code, &lines_call),
        0,
        "no code line may split text on newlines"
    );

    // The positive half: the CLI leg is handed a JSON document, not texts.
    let input_flag = needle(&["\"--in", "put\""]);
    assert!(
        count_occurrences(&code, &input_flag) >= 1,
        "the CLI leg must pass the request document with the --input flag"
    );
}

#[test]
fn parity_harness_makes_no_assertion_about_latency_being_positive() {
    let source = harness_source();
    let code = code_lines(&source);

    // The review's LOW finding: a fast operation under a coarse timer
    // legitimately reports 0.0, so `> 0` is a flake with a schedule.
    for forbidden in [
        needle(&["latency_ms() >", " 0"]),
        needle(&["latency_ms() >", "= 0.0 &&"]),
        needle(&["latency >", " 0.0"]),
        needle(&["assert!(latency", " >"]),
    ] {
        assert_eq!(
            count_occurrences(&code, &forbidden),
            0,
            "no positivity assertion on latency_ms may exist ({forbidden})"
        );
    }

    // And the finiteness check that IS legitimate must be present, or the
    // absence above would be satisfied by not looking at latency at all.
    let finite = needle(&["latency.is_", "finite()"]);
    assert!(
        count_occurrences(&code, &finite) >= 1,
        "latency is still checked for finiteness"
    );
}

#[test]
fn parity_harness_writes_no_unwrap() {
    let source = harness_source();
    let code = code_lines(&source);
    let bare_unwrap = needle(&[".unw", "rap()"]);
    assert_eq!(
        count_occurrences(&code, &bare_unwrap),
        0,
        "unwrap() is banned crate-wide (.clippy.toml disallowed-methods); the one \
         `allow(clippy::disallowed_methods)` in this file covers `serde_json::json!`'s \
         macro expansion and nothing hand-written"
    );
}

#[test]
fn parity_harness_builds_its_fixture_from_the_full_pin_shape_recipe() {
    let source = harness_source();
    let code = code_lines(&source);
    let recipe = needle(&["fixture_view_full_", "pin_shape"]);
    assert!(
        count_occurrences(&code, &recipe) >= 2,
        "the fixture must be built through the core `fixture_view_full_pin_shape()` recipe \
         (definition + call), so the parity legs run on the production nullability shape"
    );
    // The two production doors, not a hand-rolled container writer.
    for door in [
        needle(&["write_setfit_", "apr("]),
        needle(&["load_setfit_", "apr("]),
    ] {
        assert!(
            count_occurrences(&code, &door) >= 1,
            "the fixture goes through core's own door ({door})"
        );
    }
}
