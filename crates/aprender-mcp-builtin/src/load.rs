//! The once-per-process model load (design §4.1): the source comes from the configuration,
//! the bytes stream from S3 under the sha pin through the fork's `resolve_model`, the served
//! task is checked against the contract's bounds and the configuration's labels, and the
//! result is cached in a process-wide cell. `initialize`, `tools/list` and `status` never call
//! this; the first `classify` or `verify` does.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aprender_mcp_decide::{check_served_task_fits, ClassifyLimits, ClassifyService, Model};
use aprender_mcp_decide_lambda::{
    resolve_model, LoadOnce, LoadTimeline, ModelSource, ENV_S3_URI, ENV_SHA256,
};

use crate::config::ModelSection;

/// The loaded model and what the load measured.
#[derive(Debug)]
pub struct Loaded {
    pub service: ClassifyService,
    /// The artifact's labels, in task order.
    pub labels: Vec<String>,
    pub timeline: LoadTimeline,
    pub performed_at_unix: u64,
}

/// A load refusal: a short stable `kind` (the fork's `ResolveError::kind()` values, plus
/// `config_error` and `labels_mismatch`) and the detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadFailure {
    pub kind: &'static str,
    pub detail: String,
}

/// The S3 source the configuration names, through the fork's lookup-based constructor.
///
/// # Errors
///
/// A [`LoadFailure`] of kind `config_error` when the URI or pin is malformed (the strict
/// parser has already refused those, so this is belt and braces).
pub fn source_from_config(model: &ModelSection) -> Result<ModelSource, LoadFailure> {
    ModelSource::from_lookup(|name| match name {
        ENV_S3_URI => Some(model.artifact_uri.clone()),
        ENV_SHA256 => Some(model.artifact_sha256.clone()),
        _ => None,
    })
    .map_err(|e| LoadFailure {
        kind: "config_error",
        detail: e.to_string(),
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Resolve the artifact, check the served task fits the contract and the configuration's
/// labels, and build the classify service.
///
/// # Errors
///
/// A [`LoadFailure`] naming the step that refused; the caller retries on the next call.
pub async fn load(
    source: &ModelSource,
    expected_labels: Option<&[String]>,
) -> Result<Loaded, LoadFailure> {
    let (model, timeline) = resolve_model(source).await.map_err(|e| LoadFailure {
        kind: e.kind(),
        detail: e.to_string(),
    })?;
    let model: Arc<Model> = Arc::new(model);
    // Moved from build time to load time for this type (design §6.4): the model is not known
    // when the server is built.
    check_served_task_fits(&model, &ClassifyLimits::CONTRACTED).map_err(|e| LoadFailure {
        kind: "config_error",
        detail: e.to_string(),
    })?;
    let labels: Vec<String> = model.labels().iter().map(|l| (*l).to_string()).collect();
    if let Some(expected) = expected_labels {
        if expected != labels.as_slice() {
            return Err(LoadFailure {
                kind: "labels_mismatch",
                detail: format!("config labels {expected:?} are not the artifact's {labels:?}"),
            });
        }
    }
    Ok(Loaded {
        service: ClassifyService::served(model),
        labels,
        timeline,
        performed_at_unix: now_unix(),
    })
}

/// A once-per-process slot for the loaded model.
#[derive(Debug, Default)]
pub struct ModelCell(LoadOnce<Loaded>);

impl ModelCell {
    /// An empty slot.
    #[must_use]
    pub const fn new() -> Self {
        Self(LoadOnce::new())
    }

    /// The loaded model, if any.
    #[must_use]
    pub fn get(&self) -> Option<Arc<Loaded>> {
        self.0.get()
    }

    /// The loaded model, loading it on the first successful call. The flag is `true` only
    /// for the call that performed the load.
    ///
    /// # Errors
    ///
    /// The [`LoadFailure`]; the slot stays empty so the next call retries.
    pub async fn get_or_load(
        &self,
        source: &ModelSource,
        expected_labels: Option<&[String]>,
    ) -> Result<(Arc<Loaded>, bool), LoadFailure> {
        self.0
            .get_or_try_load(|| load(source, expected_labels))
            .await
    }
}

/// The process-wide cell the deployed server uses.
pub static MODEL: ModelCell = ModelCell::new();
