//! `POST /v1/classify` — the SetFit classification surface (D-09, OPS-05).
//!
//! # This module is TRANSPORT. All of it.
//!
//! CLAUDE.md's realizar-first table carries one deliberate exception, and this is
//! it: SetFit classification is `aprender-core`'s, because core's fixture-verified
//! graph path is the only conformance-proven implementation there is. What lives
//! here is the route, the `AppState` slot and the readiness fields. What does NOT
//! live here — and must never — is a tokenizer, a pooling rule, a head, or a
//! request or response schema.
//!
//! Concretely: the extractor is core's [`ClassifyRequestDocument`] and the success
//! body is core's [`ClassifyResponse`]. The ONLY struct this module serializes
//! that it did not get from core is the pre-existing transport
//! [`ErrorResponse`](super::ErrorResponse), which every other handler in this
//! directory already uses. A serve-local request or response type would make "the
//! Rust API, the CLI and HTTP agree" a claim about three independently-maintained
//! structs; routing core's own types through the wire makes it true by
//! construction (classify.rs's module header states the same rule from the other
//! side).
//!
//! # Why the bounds are checked here AND in core
//!
//! [`MAX_BATCH_TEXTS`] is re-checked by `VerifiedSetFitModel::classify` before it
//! tokenizes anything. That is not a reason to omit it here — it is the reason
//! the check here uses core's CONSTANT rather than a literal `256`. Two numbers
//! would be two bounds; one constant checked twice is defense in depth (T-04-23).

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use aprender::setfit::classify::{
    ClassifyError, ClassifyRequestDocument, ClassifyResponse, MAX_BATCH_TEXTS,
    MAX_REQUEST_BODY_BYTES,
};
use aprender::setfit::VerifiedSetFitModel;

use super::{AppState, ErrorResponse};

// ---------------------------------------------------------------------------
// The AppState slot
// ---------------------------------------------------------------------------

impl AppState {
    /// Install a verified SetFit classifier.
    ///
    /// A consuming builder in the shape of `with_verbose`/`with_trace`, so a
    /// state may hold a classifier alongside anything else it already holds.
    /// Composed with [`AppState::default`] it spells the classifier-only server
    /// `apr serve` builds for a SetFit APR.
    ///
    /// There is no `set_` variant and no way to install an unverified model: the
    /// parameter type is the witness, and it is not constructible outside
    /// `aprender-core` (APR-04).
    #[must_use]
    pub fn with_setfit_model(mut self, model: Arc<VerifiedSetFitModel>) -> Self {
        self.setfit_model = Some(model);
        self
    }

    /// The resident classifier, if any.
    #[must_use]
    pub fn setfit_model(&self) -> Option<&Arc<VerifiedSetFitModel>> {
        self.setfit_model.as_ref()
    }

    /// Whether a verified classifier is resident.
    #[must_use]
    pub fn has_setfit_model(&self) -> bool {
        self.setfit_model.is_some()
    }
}

// ---------------------------------------------------------------------------
// The route's body bound
// ---------------------------------------------------------------------------

/// The contract's `max_request_body_bytes`, as axum's layer wants it.
///
/// A function and not a `const` because the contract states the bound in `u64`
/// and the layer takes `usize`; `unwrap_or(usize::MAX)` is unreachable on every
/// target this crate builds for (a 16-bit target could not host axum), and
/// saturating UP rather than panicking is the right failure direction for a
/// transport bound whose real enforcement is core's.
#[must_use]
pub(crate) fn classify_body_limit_bytes() -> usize {
    usize::try_from(MAX_REQUEST_BODY_BYTES).unwrap_or(usize::MAX)
}

// ---------------------------------------------------------------------------
// The handler
// ---------------------------------------------------------------------------

/// A typed transport refusal.
fn refuse(code: StatusCode, message: String) -> (StatusCode, Json<ErrorResponse>) {
    (code, Json(ErrorResponse { error: message }))
}

/// Map a [`ClassifyError`] onto a status.
///
/// The split is by WHOSE fault it is, not by convenience:
///
/// * the request's shape — 400, the client can fix it by sending something else;
/// * everything else — 500, because a verified model failing to encode, or an
///   envelope constructor refusing a value the model produced, is this server's
///   problem and a client retrying the same body will not help.
///
/// The wildcard is mandatory (`ClassifyError` is `#[non_exhaustive]`) and lands
/// on 500 deliberately: a variant this build has never heard of is an internal
/// condition, and guessing 400 would tell a client to change a body that is fine.
fn classify_error_response(error: &ClassifyError) -> (StatusCode, Json<ErrorResponse>) {
    let code = match error {
        ClassifyError::EmptyInput
        | ClassifyError::BatchTooLarge { .. }
        | ClassifyError::UnsupportedSchemaVersion { .. } => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refuse(code, error.to_string())
}

/// `POST /v1/classify` — classify an ordered batch of texts.
///
/// The route exists whenever the `setfit` feature is compiled in, so a server
/// with no classifier resident answers **503**, never 404 (review M4). See the
/// installation site in `router.rs` for why that distinction is load-bearing.
///
/// Order of business, and the order matters:
///
/// 1. the slot — a request that cannot be served at all is refused before its
///    contents are judged, so a client is never told its body is wrong when the
///    real answer is "this server has no classifier";
/// 2. the request bounds, using core's constants;
/// 3. core's one classify path.
///
/// The response is core's envelope, serialized by core's own `Serialize`. This
/// function neither builds nor rewrites it, which is what makes the CLI's JSON
/// and this body the same bytes for the same model and input.
pub(crate) async fn setfit_classify_handler(
    State(state): State<AppState>,
    Json(request): Json<ClassifyRequestDocument>,
) -> Result<Json<ClassifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let model = state.setfit_model.as_ref().ok_or_else(|| {
        refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "no SetFit model is loaded: this server was started without a setfit-apr-v1 \
             artifact, so /v1/classify exists but cannot be served"
                .to_string(),
        )
    })?;

    if request.texts.is_empty() {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            ClassifyError::EmptyInput.to_string(),
        ));
    }
    if request.texts.len() > MAX_BATCH_TEXTS {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            ClassifyError::BatchTooLarge {
                max: MAX_BATCH_TEXTS,
                got: request.texts.len(),
            }
            .to_string(),
        ));
    }

    model
        .classify(&request)
        .map(Json)
        .map_err(|e| classify_error_response(&e))
}
