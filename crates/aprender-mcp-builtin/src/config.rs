//! The `config.toml` of an `aprender` server (design §3), parsed strictly: every section is
//! `deny_unknown_fields`, every rule is a named error, and no required field has a default.
//! Ported from the platform's spike 036 prototype with the lowercase id grammar.

use serde::Deserialize;
use std::fmt;

/// The only schema this crate serves.
pub const SCHEMA: &str = "aprender-server/v1";
/// The `[publish].baseline_sha256` value of a first prod publish.
pub const FIRST_MODEL: &str = "first_model";
/// The default `[verify].tolerance_abs` (the A2 fine-tuned floor, laya-parity-v1).
pub const DEFAULT_TOLERANCE_ABS: f64 = 5.0e-5;
/// The default `[verify].default_rows` (the classify door's `classify_max_texts`).
pub const DEFAULT_ROWS: u32 = 8;
/// The default `[tools].verify_required_role`.
pub const DEFAULT_VERIFY_ROLE: &str = "admin";

/// The parsed, validated configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub id: String,
    pub schema: String,
    pub environment: Environment,
    pub model: ModelSection,
    pub records: RecordsSection,
    pub publish: PublishSection,
    #[serde(default)]
    pub hold: Option<HoldSection>,
    #[serde(default)]
    pub tools: ToolsSection,
    #[serde(default)]
    pub verify: VerifySection,
}

/// Which rule set the precheck applies and what `status` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Staging,
    Prod,
}

impl Environment {
    /// The lowercase name, as written in the configuration and the records.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }
}

/// `[publish].kind`: forward to a new model, or back to one this server served before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublishKind {
    #[default]
    Promote,
    Rollback,
}

impl PublishKind {
    /// The lowercase name, as written in the configuration and the `ELIGIBLE` line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::Rollback => "rollback",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSection {
    pub kind: String,
    pub model_id: String,
    pub artifact_uri: String,
    pub artifact_sha256: String,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordsSection {
    pub pack_outcome_uri: String,
    pub eval_jsonl_uri: String,
    pub eval_probs_uri: String,
    #[serde(default)]
    pub rescore_noise_uri: Option<String>,
    #[serde(default)]
    pub comparison_uri: Option<String>,
    #[serde(default)]
    pub staging_verification_uri: Option<String>,
    #[serde(default)]
    pub verification_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishSection {
    #[serde(default)]
    pub kind: PublishKind,
    pub reason: String,
    #[serde(default)]
    pub baseline_sha256: Option<String>,
    #[serde(default)]
    pub clears_hold: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldSection {
    pub reason: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsSection {
    #[serde(default)]
    pub verify_required_role: Option<String>,
    #[serde(default)]
    pub verify_allowed_principals: Vec<String>,
    #[serde(default)]
    pub status_required_role: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifySection {
    #[serde(default)]
    pub tolerance_abs: Option<f64>,
    #[serde(default)]
    pub default_rows: Option<u32>,
}

/// A configuration refusal: the key it names and why. Displays as
/// `config-invalid: <key> <detail>`, the precheck's `config-invalid` detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub key: String,
    pub detail: String,
}

impl ConfigError {
    fn new(key: &str, detail: impl Into<String>) -> Self {
        Self {
            key: key.to_string(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config-invalid: {} {}", self.key, self.detail)
    }
}

impl std::error::Error for ConfigError {}

/// The key a serde failure points at, spelled as the named rules spell theirs
/// (`[section].key`, or `key` at the top level): the line holding the error's span, its
/// text before `=`, and the last `[section]` header above it. `toml` when there is no span.
fn span_key(text: &str, span: Option<std::ops::Range<usize>>, message: &str) -> String {
    let Some(span) = span else {
        return "toml".to_string();
    };
    // A missing field: serde spans the TABLE that lacks it (the root, or a `[section]`
    // header) and names the field in the message; the key is that field, in that table.
    if let Some(field) = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next())
    {
        let spanned = text.get(span.clone()).unwrap_or("").trim_start();
        let section = spanned
            .strip_prefix('[')
            .and_then(|s| s.split(']').next())
            .map(str::trim);
        return match section {
            Some(s) if !s.is_empty() => format!("[{s}].{field}"),
            _ => field.to_string(),
        };
    }
    let start = span.start.min(text.len());
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let line = text[line_start..].lines().next().unwrap_or("").trim();
    let section = text[..line_start]
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with('[') && l.ends_with(']'))
        .map(|l| {
            l.trim_start_matches("[[")
                .trim_start_matches('[')
                .trim_end_matches("]]")
                .trim_end_matches(']')
                .to_string()
        });
    if line.starts_with('[') {
        return line.to_string();
    }
    let key = line.split('=').next().unwrap_or("").trim();
    if key.is_empty() {
        return "toml".to_string();
    }
    match section {
        Some(s) => format!("[{s}].{key}"),
        None => key.to_string(),
    }
}

/// Collapse every run of whitespace, including newlines, to one space: a detail must never
/// add a line to the precheck's one-verdict-line grammar (toml's messages can span two).
#[must_use]
pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 64 lowercase hex characters.
#[must_use]
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `^[a-z][a-z0-9-]{0,62}$`: lowercase because the platform slugifies ids.
fn is_server_id(s: &str) -> bool {
    let mut b = s.bytes();
    match b.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    s.len() <= 63 && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// `^[a-z0-9][a-z0-9._-]{0,63}$`.
fn is_model_id(s: &str) -> bool {
    let mut b = s.bytes();
    match b.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    s.len() <= 64
        && b.all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-')
        })
}

/// `s3://bucket/key` with both parts non-empty.
fn is_s3_uri(s: &str) -> bool {
    s.strip_prefix("s3://").is_some_and(|r| {
        r.split_once('/')
            .is_some_and(|(b, k)| !b.is_empty() && !k.is_empty())
    })
}

/// Parse and validate (design §3.2). `expected_id` is the template's substituted `SERVER_ID`;
/// a configuration whose `id` differs is `config-invalid` (the platform derives both from the
/// same line, so a difference means a hand edit).
///
/// # Errors
///
/// A [`ConfigError`] naming the first offending key.
pub fn parse(toml_text: &str, expected_id: &str) -> Result<Config, ConfigError> {
    let c: Config = toml::from_str(toml_text).map_err(|e| {
        ConfigError::new(
            &span_key(toml_text, e.span(), e.message()),
            one_line(e.message()),
        )
    })?;
    if !is_server_id(&c.id) {
        return Err(ConfigError::new(
            "id",
            "must match ^[a-z][a-z0-9-]{0,62}$ (lowercase, at most 63 characters)",
        ));
    }
    if c.id != expected_id {
        return Err(ConfigError::new(
            "id",
            format!("{:?} is not the template's SERVER_ID {expected_id:?}", c.id),
        ));
    }
    if c.schema != SCHEMA {
        return Err(ConfigError::new("schema", format!("must be {SCHEMA:?}")));
    }
    if c.model.kind != "decide" {
        return Err(ConfigError::new(
            "[model].kind",
            format!("{:?} is not decide (the only kind in v1)", c.model.kind),
        ));
    }
    if !is_model_id(&c.model.model_id) {
        return Err(ConfigError::new(
            "[model].model_id",
            "must match ^[a-z0-9][a-z0-9._-]{0,63}$",
        ));
    }
    if !is_s3_uri(&c.model.artifact_uri) || !c.model.artifact_uri.ends_with(".apr") {
        return Err(ConfigError::new(
            "[model].artifact_uri",
            "must be s3://bucket/key.apr",
        ));
    }
    if !is_sha256_hex(&c.model.artifact_sha256) {
        return Err(ConfigError::new(
            "[model].artifact_sha256",
            "must be 64 lowercase hex",
        ));
    }
    if c.model
        .description
        .as_deref()
        .is_some_and(|d| d.chars().count() > 500)
    {
        return Err(ConfigError::new(
            "[model].description",
            "is over 500 characters",
        ));
    }
    for (name, uri) in [
        ("pack_outcome_uri", Some(&c.records.pack_outcome_uri)),
        ("eval_jsonl_uri", Some(&c.records.eval_jsonl_uri)),
        ("eval_probs_uri", Some(&c.records.eval_probs_uri)),
        ("comparison_uri", c.records.comparison_uri.as_ref()),
        (
            "staging_verification_uri",
            c.records.staging_verification_uri.as_ref(),
        ),
    ] {
        if let Some(u) = uri {
            if !is_s3_uri(u) {
                return Err(ConfigError::new(
                    &format!("[records].{name}"),
                    "must be s3://bucket/key",
                ));
            }
        }
    }
    if c.records.rescore_noise_uri.is_some() {
        // Accepted-and-ignored would be the silent no-op the strict parser exists to refuse:
        // v1's verify uses [verify].tolerance_abs only (the A2 noise bound is the pack task's).
        return Err(ConfigError::new(
            "[records].rescore_noise_uri",
            "is not read in v1 (verify uses [verify].tolerance_abs; the noise-referenced bound is the pack task's); remove the key",
        ));
    }
    if let Some(p) = &c.records.verification_prefix {
        if !is_s3_uri(p) || !p.ends_with('/') {
            return Err(ConfigError::new(
                "[records].verification_prefix",
                "must be s3://bucket/prefix/",
            ));
        }
    }
    if c.publish.reason.chars().count() > 1000 {
        return Err(ConfigError::new(
            "[publish].reason",
            "is over 1000 characters",
        ));
    }
    if let Some(b) = &c.publish.baseline_sha256 {
        if b != FIRST_MODEL && !is_sha256_hex(b) {
            return Err(ConfigError::new(
                "[publish].baseline_sha256",
                "must be 64 lowercase hex or first_model",
            ));
        }
    }
    if c.is_prod() && c.publish.baseline_sha256.is_none() {
        return Err(ConfigError::new(
            "[publish].baseline_sha256",
            "is required when environment = prod",
        ));
    }
    if let Some(t) = c.verify.tolerance_abs {
        if !(t > 0.0 && t <= 1.0e-3) {
            return Err(ConfigError::new(
                "[verify].tolerance_abs",
                "must be in (0, 1.0e-3]",
            ));
        }
    }
    if let Some(r) = c.verify.default_rows {
        if !(1..=DEFAULT_ROWS).contains(&r) {
            return Err(ConfigError::new(
                "[verify].default_rows",
                format!("must be 1..={DEFAULT_ROWS}"),
            ));
        }
    }
    Ok(c)
}

/// sha256 of the exact TOML bytes, reported by `status` and the precheck so a chat can tell
/// which publish it is talking to.
#[must_use]
pub fn config_sha256(toml_text: &str) -> String {
    aprender_mcp_decide_lambda::sha256_hex(toml_text.as_bytes())
}

impl Config {
    /// `environment = "prod"`.
    #[must_use]
    pub fn is_prod(&self) -> bool {
        self.environment == Environment::Prod
    }

    /// `[verify].tolerance_abs`, or the default.
    #[must_use]
    pub fn tolerance_abs(&self) -> f64 {
        self.verify.tolerance_abs.unwrap_or(DEFAULT_TOLERANCE_ABS)
    }

    /// `[verify].default_rows`, or the default.
    #[must_use]
    pub fn default_rows(&self) -> u32 {
        self.verify.default_rows.unwrap_or(DEFAULT_ROWS)
    }

    /// `[tools].verify_required_role`, or the default.
    #[must_use]
    pub fn verify_role(&self) -> &str {
        self.tools
            .verify_required_role
            .as_deref()
            .unwrap_or(DEFAULT_VERIFY_ROLE)
    }
}
