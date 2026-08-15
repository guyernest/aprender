//! The ONE place `apr-cli` decides "is this APR a SetFit classifier?" (D-04).
//!
//! # Detection reads the TYPED TAG and nothing else
//!
//! `model_type == "setfit"` in the APR v2 metadata record, plus the single custom
//! key `"setfit"` that carries the artifact document. Tensor names are NEVER
//! consulted. That is D-04's negative stated as code: a file that happens to carry
//! `setfit.head.weight` — a hand-assembled APR, a partially converted checkpoint, a
//! deliberately shaped decoy — is a plain APR to every command here, because the
//! only thing that makes an artifact a SetFit artifact is that its writer said so in
//! the field reserved for saying so.
//!
//! Sniffing by tensor name would make the classification a GUESS about content, and
//! a guess that lands on `load_setfit_apr` hands hostile bytes to a rebuild path
//! that a plain `apr inspect` would have merely described.
//!
//! # This is a HEADER + METADATA read, never a tensor load
//!
//! Detection costs 64 bytes plus the metadata block. `apr predict` on a 200 MB
//! artifact that turns out not to be a classifier must not have paid for the
//! tensors to find that out, and `apr inspect` — which never classifies — must not
//! pay for them at all.
//!
//! # The metadata length is bounded by the FILE, not believed
//!
//! `metadata_size` is a `u32` read out of the file being judged, so an attacker
//! controls it up to 4 GiB. It is checked against the stat'd file length before the
//! buffer is allocated: a block cannot be longer than the file that contains it, and
//! that bound needs no invented constant to be true. `read_exact` failing afterwards
//! is a diagnosis, not a defence — by then the allocation has already happened.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use aprender::format::v2::{AprV2Header, AprV2Metadata, HEADER_SIZE_V2, MAGIC_V2};

use crate::error::CliError;

/// The value `AprV2Metadata::model_type` carries for a `setfit-apr-v1` artifact.
pub(crate) const SETFIT_MODEL_TYPE: &str = "setfit";

/// The single custom metadata key the artifact document lives at.
pub(crate) const SETFIT_CUSTOM_KEY: &str = "setfit";

/// What the tag read recovered.
///
/// `doc` is `Option` because the two facts are independent: the tag is what ROUTES,
/// and the document is what a reader then renders. An artifact tagged `setfit` whose
/// custom key is missing is still routed to the SetFit path — where the loader
/// refuses it by name — rather than silently falling back to "plain APR", which
/// would report a corrupt classifier as a healthy generic model.
#[derive(Debug, Clone)]
pub(crate) struct SetFitTag {
    /// The raw value at custom key [`SETFIT_CUSTOM_KEY`], unparsed.
    ///
    /// Deliberately `serde_json::Value` and not a typed document: this module is
    /// compiled with the `setfit` feature OFF as well, where `SetFitArtifactDoc`
    /// cannot be named. Callers that have the feature parse it; `apr inspect`
    /// renders the raw value so identity fields stay recoverable from a binary
    /// built without the classifier.
    pub(crate) doc: Option<serde_json::Value>,
}

/// Read a file's typed SetFit tag, if it has one.
///
/// Returns `Ok(None)` for anything that is readable but not a tagged SetFit APR —
/// a GGUF, a SafeTensors file, a legacy APR, a plain APR v2, or an APR v2 whose
/// metadata block does not parse. Deciding what to say about those belongs to the
/// command, which knows whether it can do something useful with a plain APR.
///
/// # Errors
///
/// [`CliError::FileNotFound`] for an absent path, [`CliError::NotAFile`] for a
/// directory or other non-regular file, [`CliError::InvalidFormat`] when the
/// metadata block the header declares does not fit inside the file, and
/// [`CliError::Io`] for a read failure.
pub(crate) fn read_setfit_tag(path: &Path) -> Result<Option<SetFitTag>, CliError> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::FileNotFound(path.to_path_buf())
        } else {
            CliError::Io(error)
        }
    })?;
    if !metadata.is_file() {
        return Err(CliError::NotAFile(path.to_path_buf()));
    }
    let file_len = metadata.len();

    let mut file = fs::File::open(path).map_err(CliError::Io)?;

    // (1) THE HEADER. A file too short to hold one is not an APR v2 container, which
    //     is a "no" rather than an error: `apr predict` says the same thing about a
    //     GGUF, and both answers are "this is not a SetFit artifact".
    let mut header_bytes = [0_u8; HEADER_SIZE_V2];
    if file.read_exact(&mut header_bytes).is_err() {
        return Ok(None);
    }
    let magic = &header_bytes[0..4];
    if magic != MAGIC_V2 {
        return Ok(None);
    }
    let Ok(header) = AprV2Header::from_bytes(&header_bytes) else {
        return Ok(None);
    };
    if header.metadata_size == 0 {
        return Ok(None);
    }

    // (2) THE BOUND, BEFORE THE ALLOCATION. `metadata_size` is a u32 out of the file
    //     under judgement; a block that claims to run past the end of its own file is
    //     a malformed container, and saying so costs one comparison instead of up to
    //     4 GiB of zeroed memory.
    let declared = u64::from(header.metadata_size);
    let end = header.metadata_offset.saturating_add(declared);
    if end > file_len {
        return Err(CliError::InvalidFormat(format!(
            "{}: the metadata block declares {declared} bytes at offset {} , which runs past the \
             end of a {file_len}-byte file",
            path.display(),
            header.metadata_offset
        )));
    }

    // (3) THE READ, at the size the bound just approved.
    if file.seek(SeekFrom::Start(header.metadata_offset)).is_err() {
        return Ok(None);
    }
    let mut metadata_bytes = vec![0_u8; declared as usize];
    if file.read_exact(&mut metadata_bytes).is_err() {
        return Ok(None);
    }

    // (4) THE TAG. Parse failures are "not a SetFit artifact", not an error: a plain
    //     APR whose metadata this build cannot parse is exactly as un-SetFit as one
    //     whose `model_type` is `qwen2`.
    let Ok(meta) = AprV2Metadata::from_json(&metadata_bytes) else {
        return Ok(None);
    };
    if meta.model_type != SETFIT_MODEL_TYPE {
        return Ok(None);
    }
    Ok(Some(SetFitTag {
        doc: meta.custom.get(SETFIT_CUSTOM_KEY).cloned(),
    }))
}

/// The ONE fixture builder for tag-detection tests, shared across command modules.
///
/// It lives here rather than in each test module because the two files it produces
/// differ ONLY in `model_type`: a copy in another module would be free to drift in
/// exactly the dimension every one of these tests is sensitive to, and the drifted
/// copy would keep passing.
#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::HashMap;
    use std::io::Write as _;
    use std::path::{Path, PathBuf};

    use aprender::format::v2::{AprV2Metadata, AprV2Writer};

    use super::SETFIT_CUSTOM_KEY;

    /// An APR v2 container carrying SetFit-SHAPED tensors and a caller-chosen tag.
    ///
    /// The tensor names are the ones a real SetFit artifact uses, so a detector that
    /// sniffed content rather than reading the tag would classify the untagged case
    /// as SetFit — which is the D-04 negative these fixtures exist to express.
    ///
    /// The container is REAL (written by the production writer) but the tensors are
    /// not a model: nothing here can pass the load ladder, and nothing here claims
    /// to. Tag detection and load refusal are what these fixtures test.
    pub(crate) fn write_setfit_shaped_apr(
        dir: &Path,
        name: &str,
        model_type: &str,
        doc: Option<&str>,
    ) -> PathBuf {
        let mut custom: HashMap<String, serde_json::Value> = HashMap::new();
        if let Some(doc) = doc {
            custom.insert(
                SETFIT_CUSTOM_KEY.to_string(),
                serde_json::from_str(doc).expect("the fixture document is valid JSON"),
            );
        }
        let metadata = AprV2Metadata {
            model_type: model_type.to_string(),
            created_at: None,
            custom,
            ..Default::default()
        };
        let mut writer = AprV2Writer::new(metadata);
        writer.add_f32_tensor("setfit.head.weight".to_string(), vec![2, 4], &[0.5_f32; 8]);
        writer.add_f32_tensor("setfit.head.bias".to_string(), vec![2], &[0.0_f32; 2]);
        let bytes = writer.write().expect("the fixture container is writable");
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("fixture file is creatable");
        file.write_all(&bytes).expect("fixture file is writable");
        file.sync_all().expect("fixture file syncs");
        path
    }
}

#[cfg(test)]
#[path = "setfit_tag_tests.rs"]
mod setfit_tag_tests;
