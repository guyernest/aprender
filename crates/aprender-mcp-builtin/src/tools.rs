//! The three tools (design §4.2–§4.4): `classify` loads the model lazily on its first call,
//! `status` never loads, `verify` re-scores frozen evaluation rows behind a door. Ported from
//! the platform's spike 033 harness handlers, which were measured live behind the proxy.

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use aprender_mcp_decide::{parse_args, ClassifyArgs, ClassifyLimits, TOOL_NAME};
use aprender_mcp_decide_lambda::load_log_line;
use pmcp::{RequestHandlerExtra, SimpleTool, SimpleToolExt, ToolHandler};

use crate::config::ModelSection;
use crate::server::Runtime;

/// The boxed future a [`pmcp::SimpleTool`] handler returns.
pub type ToolFuture = Pin<Box<dyn Future<Output = pmcp::Result<serde_json::Value>> + Send>>;

/// The `classify` description, from the configuration (never from the model, which is not
/// loaded when `tools/list` is answered): `[model].description`, then the bounds and labels.
#[must_use]
pub fn classify_description(model: &ModelSection) -> String {
    let l = ClassifyLimits::CONTRACTED;
    let mut s = String::new();
    if let Some(d) = &model.description {
        s.push_str(d);
        s.push(' ');
    }
    s.push_str(&format!(
        "Classify 1..{} texts (each at most {} bytes, {} built tokens per call) with the \
         {} decider; labels: {}. The model loads on the first call of a fresh container.",
        l.max_texts,
        l.max_text_bytes,
        l.max_total_tokens,
        model.model_id,
        model
            .labels
            .as_ref()
            .map_or_else(|| "(from the artifact)".to_string(), |v| v.join(", ")),
    ));
    s
}

/// The load failure as the caller sees it: the kind and the detail, and that the next call
/// retries (the cell stays empty).
fn load_error(f: &crate::load::LoadFailure) -> pmcp::Error {
    pmcp::Error::internal(format!(
        "model load failed ({}): {}; the next call retries",
        f.kind, f.detail
    ))
}

/// `classify`: bounds first (an oversized or malformed request is refused before it can
/// trigger a cold load), then the once-per-process load, then the contracted service.
pub fn classify_tool(rt: Runtime) -> impl ToolHandler + 'static {
    let description = classify_description(&rt.config.model);
    SimpleTool::new(
        TOOL_NAME,
        move |args: serde_json::Value, _extra: RequestHandlerExtra| -> ToolFuture {
            let rt = rt.clone();
            Box::pin(async move {
                let parsed = parse_args(args)?;
                let started = Instant::now();
                let (loaded, performed) = rt
                    .cell
                    .get_or_load(&rt.source, rt.config.model.labels.as_deref())
                    .await
                    .map_err(|f| load_error(&f))?;
                let load_ms = started.elapsed().as_millis();
                tracing::info!(
                    "{}",
                    load_log_line(
                        performed,
                        None,
                        load_ms,
                        performed.then_some(&loaded.timeline)
                    )
                );
                let response = loaded.service.call(parsed).await?;
                serde_json::to_value(&response)
                    .map_err(|e| pmcp::Error::internal(format!("response serialization: {e}")))
            })
        },
    )
    .with_description(description)
    .with_schema_from::<ClassifyArgs>()
}

pub use args::StatusArgs;

/// The argument types and their schema derives: the only non-test place in this crate where
/// `clippy::disallowed_methods` is allowed. schemars' `JsonSchema` derive expands to
/// `.unwrap()` and its impl lands at module scope, where a struct-level allow cannot reach
/// it (the `aprender-mcp-decide` precedent, IN-05).
mod args {
    #![allow(clippy::disallowed_methods)]

    use serde::Deserialize;

    /// `status` takes no arguments.
    #[derive(Debug, Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct StatusArgs {}
}

/// The non-secret part of the verified caller (`extra.auth_context`, which pmcp fills from
/// the proxy's `x-pmcp-user-*` headers; never a token), or `null` without an identity.
#[must_use]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
pub fn identity_json(extra: &RequestHandlerExtra) -> serde_json::Value {
    extra
        .auth_context
        .as_ref()
        .map_or(serde_json::Value::Null, |c| {
            serde_json::json!({
                "subject": c.subject,
                "email": c.email(),
                "groups": c.groups(),
            })
        })
}

/// `status`: the configuration, the load state and the build identity. Never loads.
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
pub fn status_tool(rt: Runtime) -> impl ToolHandler + 'static {
    SimpleTool::new(
        "status",
        move |_args: serde_json::Value, extra: RequestHandlerExtra| -> ToolFuture {
            let rt = rt.clone();
            Box::pin(async move {
                let cfg = &rt.config;
                let l = ClassifyLimits::CONTRACTED;
                let load = rt.cell.get().map(|x| {
                    serde_json::json!({
                        "performed_at_unix": x.performed_at_unix,
                        "download_ms": x.timeline.fetch_ms,
                        "sha_ms": x.timeline.sha_ms,
                        "build_ms": x.timeline.build_ms,
                        "bytes": x.timeline.bytes,
                        "rss_mb": x.timeline.rss_mb,
                        "peak_rss_mb": x.timeline.peak_rss_mb,
                        "graviton": x.timeline.graviton,
                        "source": x.timeline.source,
                        "artifact_sha256": x.timeline.artifact_sha256,
                    })
                });
                Ok(serde_json::json!({
                    "schema": "aprender-status/v1",
                    "server": cfg.id,
                    "environment": cfg.environment.as_str(),
                    "model": {
                        "kind": cfg.model.kind,
                        "model_id": cfg.model.model_id,
                        "artifact_sha256": cfg.model.artifact_sha256,
                        "labels": cfg.model.labels,
                    },
                    "loaded": load.is_some(),
                    "load": load,
                    "bounds": {
                        "max_texts": l.max_texts,
                        "max_text_bytes": l.max_text_bytes,
                        "max_total_tokens": l.max_total_tokens,
                        "max_in_flight": l.max_in_flight,
                        "max_pending": l.max_pending,
                    },
                    "publish": {
                        "kind": cfg.publish.kind.as_str(),
                        "reason": cfg.publish.reason,
                        "baseline_sha256": cfg.publish.baseline_sha256,
                    },
                    "hold": cfg.hold.as_ref().map(|h| serde_json::json!({ "reason": h.reason })),
                    "records": {
                        "pack_outcome_uri": cfg.records.pack_outcome_uri,
                        "eval_jsonl_uri": cfg.records.eval_jsonl_uri,
                        "eval_probs_uri": cfg.records.eval_probs_uri,
                        "verification_prefix": cfg.records.verification_prefix,
                    },
                    "identity": identity_json(&extra),
                    "build": {
                        "build_id": rt.build_id,
                        "config_sha256": rt.config_sha256,
                        "crate_version": env!("CARGO_PKG_VERSION"),
                    },
                }))
            })
        },
    )
    .with_description(
        "Configuration, load state and build identity of this server. Never loads the model.",
    )
    .with_schema_from::<StatusArgs>()
}
