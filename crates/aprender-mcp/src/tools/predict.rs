//! `apr.predict` — subprocess wrapper over `apr predict <model> --text ... --json`.
//!
//! Follows the M2 pattern established by `apr.validate`: spawn the CLI with
//! `--json`, pass stdout through verbatim, map non-zero exit to `isError`.
//!
//! Deliberately GENERIC, mirroring the CLI's D-06 decision: a `setfit-apr-v1`
//! classifier is an APR like any other, so it is predicted with the same
//! command as any other model. Routing reads the artifact's typed `model_type`
//! tag and NEVER tensor names, which means a file that merely *looks* like a
//! classifier is refused by the CLI rather than silently scored — that refusal
//! reaches the MCP client unchanged as `isError: true`.
//!
//! Batch shape is intentional. `ClassifyResponse` is batch-shaped and the CLI
//! deliberately rejects line-delimited input (a line-delimited file cannot
//! carry a text containing a newline, so CLI and HTTP would receive different
//! ordered inputs while appearing to agree). One call classifies N texts;
//! response order is request order.

#![allow(clippy::disallowed_methods)] // serde_json::json! macro expands to .unwrap() internally

use crate::tools::subprocess::run_apr;
use crate::types::{InputSchema, ToolCallResult, ToolDefinition};

/// Tool name registered with MCP clients.
pub const NAME: &str = "apr.predict";

/// Return the MCP tool definition for `apr.predict`.
///
/// FALSIFY-MCP-008: `inputSchema` and `description` come from build-time
/// codegen constants emitted from `contracts/apr-mcp-tool-schemas-v1.yaml`.
/// Neither may be hand-coded here.
#[must_use]
pub fn predict_tool_definition() -> ToolDefinition {
    let input_schema: InputSchema = serde_json::from_str(crate::schemas::APR_PREDICT_SCHEMA)
        .expect(
            "FALSIFY-MCP-008: apr.predict codegen constant must parse as InputSchema; \
             regenerate by editing contracts/apr-mcp-tool-schemas-v1.yaml and rebuilding",
        );
    ToolDefinition {
        name: NAME.to_string(),
        description: crate::schemas::APR_PREDICT_DESCRIPTION.to_string(),
        input_schema,
    }
}

/// Execute `apr.predict` by spawning `apr predict <model> --text <t>... --json`.
#[must_use]
pub fn call(args: &serde_json::Value) -> ToolCallResult {
    let Some(model_path) = args.get("model_path").and_then(|v| v.as_str()) else {
        return ToolCallResult::error("Missing required argument: model_path");
    };
    let Some(texts) = args.get("texts").and_then(|v| v.as_array()) else {
        return ToolCallResult::error("Missing required argument: texts (array of strings)");
    };
    if texts.is_empty() {
        // An empty batch is a client bug, not an empty result: `apr predict`
        // with no --text would read differently (and the response's ordered
        // contract would be vacuous). Refuse here rather than spawn.
        return ToolCallResult::error("Argument `texts` must contain at least one text");
    }
    // Reject a non-string element instead of lossily stringifying it — a
    // silently coerced `42` would be classified as the literal "42" and the
    // caller would never learn their input was not what they sent.
    let mut owned: Vec<String> = Vec::with_capacity(texts.len());
    for (index, value) in texts.iter().enumerate() {
        match value.as_str() {
            Some(text) => owned.push(text.to_string()),
            None => {
                return ToolCallResult::error(format!(
                    "Argument `texts[{index}]` must be a string, got: {value}"
                ));
            }
        }
    }

    let include_logits = args
        .get("include_logits")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let mut argv: Vec<&str> = vec!["predict", model_path];
    for text in &owned {
        argv.push("--text");
        argv.push(text.as_str());
    }
    if include_logits {
        argv.push("--logits");
    }
    argv.push("--json");
    run_apr(&argv)
}

/// HELIX-IDEA-002 — unified-signature shim for the inventory dispatcher.
pub fn dispatch(
    args: &serde_json::Value,
    _cancel: &std::sync::mpsc::Receiver<()>,
    _sink: Option<&crate::server::NotificationSink>,
    _token: Option<serde_json::Value>,
) -> ToolCallResult {
    call(args)
}

crate::register_mcp_tool!(
    name: NAME,
    definition: predict_tool_definition,
    dispatch: dispatch,
);

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to code that hits unwrap()
mod tests {
    use super::*;

    #[test]
    fn definition_has_correct_name_and_required_fields() {
        let def = predict_tool_definition();
        assert_eq!(def.name, "apr.predict");
        assert_eq!(def.input_schema.schema_type, "object");
        assert!(def.input_schema.properties.contains_key("model_path"));
        assert!(def.input_schema.properties.contains_key("texts"));
        assert!(def
            .input_schema
            .required
            .contains(&"model_path".to_string()));
        assert!(def.input_schema.required.contains(&"texts".to_string()));
    }

    /// The `texts` schema must declare `items`; an array without it leaves the
    /// element type undefined for every client.
    #[test]
    fn texts_schema_declares_string_items() {
        let def = predict_tool_definition();
        let texts = def
            .input_schema
            .properties
            .get("texts")
            .expect("texts property must exist");
        assert_eq!(texts.prop_type, "array");
        let items = texts
            .items
            .as_ref()
            .expect("array schema must declare items");
        assert_eq!(items.item_type, "string");
    }

    #[test]
    fn missing_model_path_returns_error() {
        let result = call(&serde_json::json!({ "texts": ["hello"] }));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("model_path"));
    }

    #[test]
    fn missing_texts_returns_error() {
        let result = call(&serde_json::json!({ "model_path": "m.apr" }));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("texts"));
    }

    #[test]
    fn empty_texts_is_refused_not_spawned() {
        let result = call(&serde_json::json!({ "model_path": "m.apr", "texts": [] }));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("at least one"));
    }

    /// A non-string element must be refused, never coerced: a silently
    /// stringified `42` would be classified as the literal "42".
    #[test]
    fn non_string_text_element_is_refused() {
        let result = call(&serde_json::json!({ "model_path": "m.apr", "texts": ["ok", 42] }));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("texts[1]"));
    }
}
