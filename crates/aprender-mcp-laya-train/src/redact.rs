//! Caller-safe text (D-18, D-10, WR-03): no string a caller can read carries an
//! AWS account id, an ARN, an `amazonaws.com` host, a URL or an access key id.
//!
//! The protection sits at the boundaries where text becomes caller-visible, not
//! at each site that builds an error, so a future error path inherits it:
//!
//! | Boundary | What it does |
//! |----------|--------------|
//! | [`crate::layout::StatusPayload::to_value`] | the one serializer: `error`, `verdict.pack.line` and every `notes` entry pass through [`caller_safe`] (the stored payload, `verdict.json`, `tasks/result`, `train_status`) |
//! | [`stage_error`] | the Catch's `{Error, Cause}` is REDUCED, not forwarded: an identifier name and a whitelist of reason keys |
//! | [`RedactingBackend`] | the one wrapper around the deployment's task backend: an internal store error reaches the caller as a fixed text naming the operation, the full text goes to the server-side log |
//!
//! The dispatch error label (`start_error_label` in the Lambda crate) is the
//! fourth: an AWS error CODE, never the SDK message.
//!
//! # The rule (blacklist, so the backend and dispatch sinks use fixed or code-only text)
//!
//! [`caller_safe`] drops control characters, collapses whitespace runs to one space
//! and splits the text into spans at whitespace and at the delimiters
//! `" ' , ; ( ) [ ] { } < > = \ |`. A span is replaced by [`PLACEHOLDER`] when it
//!
//! - contains `arn:aws` in any letter case (every partition: `aws-cn`, `aws-us-gov`);
//! - contains `amazonaws.com` in any letter case (S3, ECR, STS hosts);
//! - contains `://` (a URL, a presigned link, an `s3://` URI). A URL's query string
//!   uses `=`, `,` and `;`, so the redaction runs on to the next whitespace, quote,
//!   angle bracket, brace, backslash or pipe: a signature or a security token after
//!   an `=` is never left behind;
//! - contains a maximal run of EXACTLY twelve ASCII digits (an account id) whose
//!   previous character is not a full stop and not an ASCII letter or digit, and
//!   whose next character is not an ASCII letter or digit. A float fraction (after
//!   the full stop), a run inside a hex digest and the last group of a UUID (eight,
//!   four, four and four hex characters with hyphens before it) are kept;
//! - contains `AKIA` or `ASIA` at a word start followed by sixteen or more upper-case
//!   letters or digits (an access key id).
//!
//! The result is truncated to `max_chars` characters after redaction, and a span the
//! cut leaves partial is kept only when it passes the same rule on its own, so the
//! function is idempotent: text stored once and re-served later is unchanged.
//!
//! # SK-12: the bucket name is a handle, not a leak
//!
//! The run bucket is `aprender-laya-train-<env>-<account>`, so `run_prefix` and every
//! manifest link carry the account id by construction. They are NOT redacted: the
//! caller names that bucket itself in `dataset_uri`, so the account id there is a
//! handle the caller already holds, and redacting it would break the `s3://` handle
//! `train_status` and `laya-fetch` resolve. Only the free-text fields (`error`,
//! `verdict.pack.line`, `notes`) are redacted; an `s3://` URI or a bucket-name
//! account id inside one of those reads [`PLACEHOLDER`].

use std::sync::Arc;

use aprender_mcp_setfit_train::{BackendError, StoredTask, TaskBackend};
use pmcp::server::task_store::TaskStoreError;

/// What a redacted span reads.
pub const PLACEHOLDER: &str = "[redacted]";

/// Bound of a caller-visible error or note.
pub const MAX_CALLER_TEXT_CHARS: usize = 1_024;

/// Bound of one reason value [`stage_error`] keeps from a JSON Cause.
const MAX_CAUSE_VALUE_CHARS: usize = 240;

/// Bound of a non-JSON Cause in [`stage_error`].
const MAX_CAUSE_TEXT_CHARS: usize = 300;

/// The digits of an AWS account id.
const ACCOUNT_ID_DIGITS: usize = 12;

/// The upper-case letters or digits that follow `AKIA` / `ASIA` in an access key id.
const ACCESS_KEY_TAIL: usize = 16;

/// Longest error name [`stage_error`] keeps.
const MAX_NAME_CHARS: usize = 64;

/// The Cause keys [`stage_error`] keeps (SageMaker, Lambda, ECS, Step Functions).
const CAUSE_KEYS: [&str; 8] = [
    "FailureReason",
    "errorMessage",
    "StoppedReason",
    "StopCode",
    "Reason",
    "ExitCode",
    "SecondaryStatus",
    "TrainingJobStatus",
];

/// How many reason values [`stage_error`] keeps.
const MAX_CAUSE_VALUES: usize = 4;

/// How deep [`stage_error`] walks a JSON Cause.
const MAX_CAUSE_DEPTH: usize = 4;

/// How many entries of one array [`stage_error`] walks.
const MAX_CAUSE_ARRAY: usize = 16;

fn is_delimiter(c: char) -> bool {
    matches!(
        c,
        ' ' | '"'
            | '\''
            | ','
            | ';'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '='
            | '\\'
            | '|'
    )
}

/// Where a URL's redaction stops: characters a URL cannot carry unescaped.
fn ends_url(c: char) -> bool {
    matches!(c, ' ' | '"' | '\'' | '<' | '>' | '{' | '}' | '\\' | '|')
}

/// Step 1: control characters dropped, whitespace runs collapsed to one space.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else if !c.is_control() {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// `chars[at - 24 .. at]` is `8hex-4hex-4hex-4hex-`: the run at `at` is a UUID's last group.
fn uuid_tail(chars: &[char], at: usize) -> bool {
    const SHAPE: [usize; 4] = [8, 4, 4, 4];
    let Some(start) = at.checked_sub(24) else {
        return false;
    };
    let mut i = start;
    for len in SHAPE {
        if !chars[i..i + len].iter().all(char::is_ascii_hexdigit) || chars[i + len] != '-' {
            return false;
        }
        i += len + 1;
    }
    true
}

/// An account-id-shaped run of digits (see the module doc).
fn has_account_digits(chars: &[char]) -> bool {
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let prev = start.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i).copied();
        let prev_ok = prev.is_none_or(|p| p != '.' && !p.is_ascii_alphanumeric());
        let next_ok = next.is_none_or(|n| !n.is_ascii_alphanumeric());
        if i - start == ACCOUNT_ID_DIGITS && prev_ok && next_ok && !uuid_tail(chars, start) {
            return true;
        }
    }
    false
}

/// `AKIA` / `ASIA` at a word start followed by sixteen or more upper-case letters or digits.
fn has_access_key(chars: &[char]) -> bool {
    (0..chars.len().saturating_sub(3)).any(|i| {
        let word_start = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        let prefix: String = chars[i..i + 4].iter().collect();
        word_start
            && (prefix == "AKIA" || prefix == "ASIA")
            && chars[i + 4..]
                .iter()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                .count()
                >= ACCESS_KEY_TAIL
    })
}

fn is_url(span: &str) -> bool {
    span.contains("://")
}

/// Whether one span must be replaced by [`PLACEHOLDER`].
fn sensitive(span: &str) -> bool {
    let lower = span.to_ascii_lowercase();
    let chars: Vec<char> = span.chars().collect();
    lower.contains("arn:aws")
        || lower.contains("amazonaws.com")
        || is_url(span)
        || has_account_digits(&chars)
        || has_access_key(&chars)
}

/// Step 2 and 3 over normalized text.
fn redact(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if is_delimiter(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !is_delimiter(chars[i]) {
            i += 1;
        }
        let span: String = chars[start..i].iter().collect();
        if is_url(&span) {
            // A URL's query string runs on through `=`, `,` and `;`.
            while i < chars.len() && !ends_url(chars[i]) {
                i += 1;
            }
            out.push_str(PLACEHOLDER);
        } else if sensitive(&span) {
            out.push_str(PLACEHOLDER);
        } else {
            out.push_str(&span);
        }
    }
    out
}

/// `text` as a caller may read it, at most `max_chars` characters (D-18, see the
/// module doc). Idempotent, never longer than `max_chars`, and total over UTF-8.
#[must_use]
pub fn caller_safe(text: &str, max_chars: usize) -> String {
    let redacted = redact(&normalize(text));
    let chars: Vec<char> = redacted.chars().collect();
    if chars.len() <= max_chars {
        return redacted;
    }
    let mut cut = max_chars;
    // A span the cut leaves partial: kept only if it passes the rule on its own
    // (13 digits cut to 12 would otherwise read as an account id next time).
    if !is_delimiter(chars[cut]) {
        let span_start = chars[..cut]
            .iter()
            .rposition(|c| is_delimiter(*c))
            .map_or(0, |p| p + 1);
        let partial: String = chars[span_start..cut].iter().collect();
        if sensitive(&partial) {
            cut = span_start;
        }
    }
    chars[..cut].iter().collect()
}

/// `error` when it is an identifier of at most 64 characters, else `error`.
fn error_name(error: Option<&str>) -> &str {
    match error {
        Some(name)
            if name.chars().count() <= MAX_NAME_CHARS
                && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')) =>
        {
            name
        }
        _ => "error",
    }
}

/// One whitelisted reason value (a string or a number), caller-safe and bounded.
fn keep_reason(v: &serde_json::Value, out: &mut Vec<String>) {
    let text = match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return,
    };
    let safe = caller_safe(&text, MAX_CAUSE_VALUE_CHARS);
    if !safe.is_empty() && out.len() < MAX_CAUSE_VALUES {
        out.push(safe);
    }
}

fn collect_reasons(value: &serde_json::Value, depth: usize, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map {
                if out.len() >= MAX_CAUSE_VALUES {
                    return;
                }
                if CAUSE_KEYS.contains(&key.as_str()) {
                    keep_reason(v, out);
                }
                if depth < MAX_CAUSE_DEPTH && (v.is_object() || v.is_array()) {
                    collect_reasons(v, depth + 1, out);
                }
            }
        }
        serde_json::Value::Array(items) if depth < MAX_CAUSE_DEPTH => {
            for v in items.iter().take(MAX_CAUSE_ARRAY) {
                if out.len() >= MAX_CAUSE_VALUES {
                    return;
                }
                collect_reasons(v, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// The Catch's `{Error, Cause}` REDUCED to caller-safe text (D-18): the name (see
/// [`error_name`]), a colon, and from a JSON Cause the first four values of the
/// whitelisted reason keys ([`CAUSE_KEYS`], strings or numbers, walked to depth 4
/// and 16 array entries), each through [`caller_safe`] at 240 characters and joined
/// with `; `. A non-JSON Cause goes through [`caller_safe`] at 300; an absent Cause,
/// or a JSON one with none of the keys, reads `no cause recorded`. Bounded to 1024.
/// The full text belongs in the server-side log only; this never forwards it.
#[must_use]
pub fn stage_error(error: Option<&str>, cause: Option<&str>) -> String {
    let summary = match cause {
        None => None,
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => {
                let mut reasons = Vec::new();
                collect_reasons(&value, 0, &mut reasons);
                (!reasons.is_empty()).then(|| reasons.join("; "))
            }
            Err(_) => Some(caller_safe(text, MAX_CAUSE_TEXT_CHARS)).filter(|s| !s.is_empty()),
        },
    };
    let summary = summary.unwrap_or_else(|| "no cause recorded".to_string());
    caller_safe(
        &format!("{}: {summary}", error_name(error)),
        MAX_CALLER_TEXT_CHARS,
    )
}

/// The deployment's task backend with its internal error text kept server-side
/// (WR-03, SK-05): the sibling `DynamoDbTaskBackend` renders every SDK failure,
/// table ARN and account id included, into `TaskStoreError::Internal`, and the
/// lock, `submit`, `train_status` and pmcp's own task handlers all read through
/// the backend. This one wrapper therefore covers every request-side store text.
///
/// `Conflict` is returned unchanged (the store's compare-and-swap loop needs it);
/// `NotFound`, `Expired` and `InvalidTransition` pass through (they carry task ids
/// only); an `Internal` is logged in full to stderr and returned as a fixed text
/// naming the operation.
pub struct RedactingBackend {
    inner: Arc<dyn TaskBackend>,
}

impl std::fmt::Debug for RedactingBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedactingBackend").finish_non_exhaustive()
    }
}

impl RedactingBackend {
    /// Wrap `inner`.
    #[must_use]
    pub fn new(inner: Arc<dyn TaskBackend>) -> Self {
        Self { inner }
    }

    fn map<T>(op: &'static str, result: Result<T, BackendError>) -> Result<T, BackendError> {
        match result {
            Err(BackendError::Store(TaskStoreError::Internal { message })) => {
                eprintln!("laya-train task store {op} failed (server-side only): {message}");
                Err(BackendError::Store(TaskStoreError::Internal {
                    message: format!("the task store failed on {op}"),
                }))
            }
            other => other,
        }
    }
}

#[pmcp::async_trait]
impl TaskBackend for RedactingBackend {
    async fn put_new(&self, owner_id: &str, record: StoredTask) -> Result<(), BackendError> {
        Self::map("put_new", self.inner.put_new(owner_id, record).await)
    }

    async fn get(&self, owner_id: &str, task_id: &str) -> Result<Option<StoredTask>, BackendError> {
        Self::map("get", self.inner.get(owner_id, task_id).await)
    }

    async fn cas(
        &self,
        owner_id: &str,
        task_id: &str,
        expected_version: u64,
        record: StoredTask,
    ) -> Result<(), BackendError> {
        Self::map(
            "cas",
            self.inner
                .cas(owner_id, task_id, expected_version, record)
                .await,
        )
    }

    async fn list(
        &self,
        owner_id: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<StoredTask>, Option<String>), BackendError> {
        Self::map("list", self.inner.list(owner_id, cursor).await)
    }

    async fn sweep_expired(&self) -> Result<usize, BackendError> {
        Self::map("sweep_expired", self.inner.sweep_expired().await)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use super::*;

    /// The real verify JSON line of the live smoke run (14-SMOKE-EVIDENCE.json,
    /// smoke.verdict.pack.line), byte for byte.
    const SMOKE_VERIFY_LINE: &str = r#"{"argmax":"459/459","artifact_sha256":"31ace8ea95ca0c750b6d007d0f4c68e4322d20363117f837673a936d6f696c85","deploy_eligible":true,"noise":8.147599085628343e-6,"recomputed":{"ece_post":0.06310529497194395,"ft_macro_f1":0.7249874875126133,"margin":0.315828019529616,"zs_macro_f1":0.40915946798299735},"rescore_bound":0.00003259039634251337,"rescore_max_abs":0.000027477741241455078,"shipped_seed":23,"waived":null,"zs_noise":4.70304053501458e-6,"zs_rescore_bound":0.00001881216214005832,"zs_rescore_max_abs":0.000015079975128173828}"#;

    /// The hostile account id the case table plants (never a real account).
    const ACCOUNT: &str = "987654321098";

    /// What must never survive a must-redact entry.
    const NEEDLES: [&str; 8] = [
        ACCOUNT,
        "arn:",
        "ARN:",
        "amazonaws.com",
        "://",
        "AKIAIOSFODNN7EXAMPLE",
        "ASIAY34FZKBOKMUTVV7A",
        "Signature",
    ];

    fn must_redact() -> Vec<(&'static str, String)> {
        vec![
            (
                "bare ARN",
                format!("arn:aws:iam::{ACCOUNT}:role/laya-train-execution-dev"),
            ),
            (
                "S3 ARN without an account",
                "arn:aws:s3:::aprender-laya-train-artifacts/runs/m/t".to_string(),
            ),
            (
                "China partition ARN",
                "denied on arn:aws-cn:s3:::laya-cn-bucket/key".to_string(),
            ),
            (
                "GovCloud partition ARN",
                "role ARN:AWS-US-GOV:iam::123412341234:role/x failed".to_string(),
            ),
            (
                "IAM not-authorized sentence",
                format!(
                    "AccessDeniedException: User: arn:aws:sts::{ACCOUNT}:assumed-role/\
                     laya-train-request-dev/aprender-laya-train is not authorized to perform: \
                     states:StartExecution on resource: arn:aws:states:us-east-1:{ACCOUNT}:\
                     stateMachine:laya-train-dev because no identity-based policy allows it"
                ),
            ),
            (
                "ECR image URI with a digest",
                format!(
                    "image {ACCOUNT}.dkr.ecr.us-east-1.amazonaws.com/aprender-laya-train@sha256:{}",
                    "a".repeat(64)
                ),
            ),
            (
                "presigned S3 URL with a signature",
                "get https://aprender-laya-train-dev.s3.us-east-1.amazonaws.com/runs/m/t/m.apr?\
                 X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20261030\
                 &X-Amz-Signature=0123abcd,x;y done"
                    .to_string(),
            ),
            (
                "s3 URI",
                "cannot read s3://aprender-laya-train-dev-123456789012/datasets/x/".to_string(),
            ),
            (
                "account then twelve digits",
                format!("in account {ACCOUNT}."),
            ),
            (
                "access key id AKIA",
                "key AKIAIOSFODNN7EXAMPLE was used".to_string(),
            ),
            (
                "access key id ASIA",
                "session=ASIAY34FZKBOKMUTVV7A;".to_string(),
            ),
        ]
    }

    fn must_keep() -> Vec<(&'static str, String)> {
        vec![
            ("the smoke verify line", SMOKE_VERIFY_LINE.to_string()),
            (
                "REFUSED RescoreDrift with 21-digit floats",
                "REFUSED RescoreDrift: max_abs 0.000027477741241455078 > bound \
                 0.000032590396342513370"
                    .to_string(),
            ),
            (
                "64-hex sha256 with a twelve-digit run inside",
                "sha256 31ace8ea123456789012c750b6d007d0f4c68e4322d20363117f837673a936d6"
                    .to_string(),
            ),
            (
                "64-hex sha256 opening with twelve digits",
                "123456789012ab0b6d007d0f4c68e4322d20363117f837673a936d6f696c85ab".to_string(),
            ),
            (
                "float with exactly twelve fractional digits",
                "ece_post 0.123456789012 margin 1.000000000001".to_string(),
            ),
            (
                "13-digit epoch in milliseconds",
                "at 1793389563000 ms".to_string(),
            ),
            ("11-digit number", "rows 17933895630".to_string()),
            (
                "UUID",
                "task 0f8b3c1e-4d2a-4b6e-9c71-2a5d8e9f1b34".to_string(),
            ),
            (
                "UUID whose last group is twelve digits",
                "task 00000000-0000-4000-8000-000000000001 holds the lock".to_string(),
            ),
            (
                "RFC 3339 timestamp",
                "started 2026-10-30T19:46:03.123Z".to_string(),
            ),
            ("States.TaskFailed", "States.TaskFailed".to_string()),
            (
                "container path under the input data directory",
                "cannot read /opt/ml/input/data/dataset/train.jsonl".to_string(),
            ),
            (
                "the stage-error-overridden note",
                crate::finalize::NOTE_STAGE_ERROR_OVERRIDDEN.to_string(),
            ),
            (
                "a listed-but-absent note with a benign path",
                "listed-but-absent: 1 manifest entry the job listed is not in S3 and was dropped: \
                 run/gate-report.json"
                    .to_string(),
            ),
        ]
    }

    #[test]
    fn redact_must_redact_table() {
        for (name, text) in must_redact() {
            let safe = caller_safe(&text, MAX_CALLER_TEXT_CHARS);
            for needle in NEEDLES {
                assert!(
                    !safe.contains(needle),
                    "{name}: {needle:?} survived in {safe:?}"
                );
            }
            assert!(safe.contains(PLACEHOLDER), "{name}: {safe:?}");
        }
    }

    #[test]
    fn redact_must_keep_table() {
        for (name, text) in must_keep() {
            assert_eq!(
                caller_safe(&text, MAX_CALLER_TEXT_CHARS * 4),
                text,
                "{name} was altered"
            );
        }
    }

    #[test]
    fn redact_keeps_the_rest_of_a_redacted_sentence() {
        let text = format!(
            "AccessDeniedException: User: arn:aws:sts::{ACCOUNT}:assumed-role/r/s is not \
             authorized to perform: dynamodb:PutItem"
        );
        assert_eq!(
            caller_safe(&text, MAX_CALLER_TEXT_CHARS),
            "AccessDeniedException: User: [redacted] is not authorized to perform: dynamodb:PutItem"
        );
        let verify =
            format!(r#"{{"deploy_eligible":true,"role":"arn:aws:iam::{ACCOUNT}:role/x"}}"#);
        assert_eq!(
            caller_safe(&verify, MAX_CALLER_TEXT_CHARS),
            r#"{"deploy_eligible":true,"role":"[redacted]"}"#,
            "a redacted JSON line is still JSON"
        );
    }

    /// 11, 12 and 13 digits with every class of previous and next character.
    #[test]
    fn redact_digit_boundaries() {
        // (character, a full stop, an ASCII letter or digit-like neighbour)
        let prevs = ["", " ", ".", "a", "Z", ":", "/", "-", "_", "@"];
        let nexts = ["", " ", ".", "a", "Z", ":", "/", "-", "_", "@"];
        for n in [11_usize, 12, 13] {
            let digits: String = "987654321098765".chars().take(n).collect();
            for prev in prevs {
                for next in nexts {
                    let text = format!("{prev}{digits}{next}");
                    let letter = |s: &str| s.chars().any(|c| c.is_ascii_alphabetic());
                    let want = n == 12 && prev != "." && !letter(prev) && !letter(next);
                    let got = caller_safe(&text, MAX_CALLER_TEXT_CHARS) != text;
                    assert_eq!(got, want, "{n} digits in {text:?}");
                }
            }
        }
        // A digit neighbour makes the run longer than twelve: never an account id.
        assert_eq!(caller_safe("9876543210981", 64), "9876543210981");
        assert_eq!(caller_safe("98765432109", 64), "98765432109");
        assert_eq!(caller_safe("987654321098", 64), PLACEHOLDER);
    }

    /// A tiny deterministic generator (xorshift), so the corpus is the same every run.
    struct Gen(u64);

    impl Gen {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            let n = u64::try_from(items.len()).expect("small");
            items[usize::try_from(self.next() % n).expect("small")]
        }
    }

    fn corpus() -> Vec<String> {
        let atoms = [
            "a",
            "Z",
            "0",
            "7",
            ".",
            "-",
            "_",
            ":",
            "/",
            " ",
            "\t",
            "\n",
            "\u{0}",
            "\u{7f}",
            "\"",
            "'",
            ",",
            ";",
            "(",
            ")",
            "[",
            "]",
            "{",
            "}",
            "<",
            ">",
            "=",
            "\\",
            "|",
            "é",
            "\u{1f600}",
            "\u{a0}",
            "arn:aws",
            "ARN:AWS-CN",
            "://",
            "amazonaws.com",
            "AKIA",
            "ASIA",
            "IOSFODNN7EXAMPLE",
            "987654321098",
            "12345678901",
            "1234567890123",
            "[redacted]",
            "0.123456789012",
            "00000000-0000-4000-8000-",
            "000000000001",
        ];
        let mut g = Gen(0x9e37_79b9_7f4a_7c15);
        (0..2_000)
            .map(|_| {
                let len = g.next() % 40;
                (0..len).map(|_| g.pick(&atoms)).collect()
            })
            .collect()
    }

    #[test]
    fn redact_is_idempotent_and_bounded() {
        let tables = must_redact()
            .into_iter()
            .chain(must_keep())
            .map(|(_, t)| t)
            .chain(corpus());
        for text in tables {
            for max in [0_usize, 1, 5, 11, 12, 13, 40, 240, MAX_CALLER_TEXT_CHARS] {
                let once = caller_safe(&text, max);
                assert!(once.chars().count() <= max, "{text:?} at {max}: {once:?}");
                assert_eq!(caller_safe(&once, max), once, "{text:?} at {max}");
            }
        }
    }

    #[test]
    fn redact_truncation_never_reveals_an_account_id() {
        // 13 digits are kept whole; cut to twelve they would read as an account id.
        assert_eq!(caller_safe("ok 1793389563000", 15), "ok ");
        assert_eq!(caller_safe("ok 1793389563000", 16), "ok 1793389563000");
        let long = "x".repeat(5_000);
        assert_eq!(caller_safe(&long, 300).chars().count(), 300);
        let multibyte = "é".repeat(500);
        assert_eq!(caller_safe(&multibyte, 7), "é".repeat(7));
    }

    #[test]
    fn redact_notes_through_to_value() {
        use crate::layout::{Phase, StatusPayload};
        let mut payload = StatusPayload::base("t-1", "m-1", "s3://b/runs/m-1/t-1/", Phase::Failed);
        let benign_note =
            "listed-but-absent: 1 manifest entry the job listed is not in S3 and was dropped: \
             run/gate-report.json";
        payload.notes = vec![
            crate::finalize::NOTE_STAGE_ERROR_OVERRIDDEN.to_string(),
            benign_note.to_string(),
            format!(
                "listed-but-absent: 2 manifest entries S3 does not hold were dropped: \
                 run/{ACCOUNT}/x.json, arn:aws:s3:::b/k"
            ),
        ];
        let value = payload.to_value();
        let notes: Vec<&str> = value["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .map(|n| n.as_str().expect("text"))
            .collect();
        assert_eq!(notes[0], crate::finalize::NOTE_STAGE_ERROR_OVERRIDDEN);
        assert_eq!(notes[1], benign_note);
        assert!(notes[2].starts_with("listed-but-absent"), "{}", notes[2]);
        assert!(
            !notes[2].contains(ACCOUNT) && !notes[2].contains("arn:"),
            "{}",
            notes[2]
        );
        // The run prefix is a handle (SK-12): never redacted.
        assert_eq!(value["run_prefix"], "s3://b/runs/m-1/t-1/");
    }

    #[test]
    fn redact_error_and_pack_line_through_to_value() {
        use crate::layout::{PackInfo, Phase, StatusPayload, Verdict};
        let mut payload = StatusPayload::base("t-1", "m-1", "s3://b/runs/m-1/t-1/", Phase::Failed);
        payload.error = Some(format!(
            "pack read {ACCOUNT}.dkr.ecr.us-east-1.amazonaws.com/x"
        ));
        payload.verdict = Some(Verdict {
            pass: true,
            waived: None,
            margin: None,
            ece_post: None,
            zero_shot_macro_f1: None,
            ft_macro_f1: None,
            shipped_seed: None,
            per_seed: Vec::new(),
            device_used: None,
            torch_version: None,
            recipe_id: None,
            shift_probe: None,
            apr_sha256: None,
            pack: Some(PackInfo {
                accepted: true,
                line: Some(format!(r#"{{"role":"arn:aws:iam::{ACCOUNT}:role/r"}}"#)),
            }),
        });
        let value = payload.to_value();
        assert_eq!(value["error"], "pack read [redacted]");
        assert_eq!(value["verdict"]["pack"]["line"], r#"{"role":"[redacted]"}"#);
        // Idempotent: a stored payload re-served is unchanged.
        let back: StatusPayload = serde_json::from_value(value.clone()).expect("payload");
        assert_eq!(back.to_value(), value);

        let mut benign = payload.clone();
        benign.error = Some("the trainer crashed (exit 1)".to_string());
        if let Some(pack) = benign.verdict.as_mut().and_then(|v| v.pack.as_mut()) {
            pack.line = Some(SMOKE_VERIFY_LINE.to_string());
        }
        let value = benign.to_value();
        assert_eq!(value["error"], "the trainer crashed (exit 1)");
        assert_eq!(value["verdict"]["pack"]["line"], SMOKE_VERIFY_LINE);
    }

    fn sagemaker_cause() -> String {
        serde_json::json!({
            // Keys in sorted order, so the walk reads them in the same order with
            // or without serde_json's preserve_order feature.
            "AlgorithmSpecification": {
                "TrainingImage": format!("{ACCOUNT}.dkr.ecr.us-east-1.amazonaws.com/aprender-laya-train@sha256:{}", "b".repeat(64)),
            },
            "FailureReason": format!("AlgorithmError: the job exited 1; see arn:aws:iam::{ACCOUNT}:role/r in account {ACCOUNT}"),
            "RoleArn": format!("arn:aws:iam::{ACCOUNT}:role/laya-train-execution-dev"),
            "SecondaryStatus": "Failed",
            "TrainingJobArn": format!("arn:aws:sagemaker:us-east-1:{ACCOUNT}:training-job/laya-x"),
            "TrainingJobStatus": "Failed",
        })
        .to_string()
    }

    fn assert_clean(text: &str) {
        for needle in [ACCOUNT, "arn:", "amazonaws.com", "://"] {
            assert!(!text.contains(needle), "{needle:?} in {text:?}");
        }
    }

    #[test]
    fn stage_error_table() {
        let sm = stage_error(Some("States.TaskFailed"), Some(&sagemaker_cause()));
        assert_eq!(
            sm,
            "States.TaskFailed: AlgorithmError: the job exited 1; see [redacted] in account \
             [redacted]; Failed; Failed"
        );

        let ecs = serde_json::json!({
            "Attachments": [{"Details": [{"Name": "subnetId", "Value": "subnet-1"}]}],
            "Containers": [
                {"ExitCode": 2, "Name": "pack", "Reason": format!("pulled {ACCOUNT}.dkr.ecr.us-east-1.amazonaws.com/p")},
            ],
            "StopCode": "EssentialContainerExited",
            "StoppedReason": "Essential container in task exited",
            "TaskArn": format!("arn:aws:ecs:us-east-1:{ACCOUNT}:task/c/abc"),
        })
        .to_string();
        let ecs = stage_error(Some("States.TaskFailed"), Some(&ecs));
        assert_eq!(
            ecs,
            "States.TaskFailed: 2; pulled [redacted]; EssentialContainerExited; Essential \
             container in task exited"
        );

        let plain = stage_error(
            Some("States.Timeout"),
            Some(&format!("Task timed out after 3600 s in {ACCOUNT}")),
        );
        assert_eq!(
            plain,
            "States.Timeout: Task timed out after 3600 s in [redacted]"
        );

        let long = stage_error(Some("States.TaskFailed"), Some(&"x".repeat(5_000)));
        assert!(long.starts_with("States.TaskFailed: x"), "{long}");
        assert_eq!(long.chars().count(), "States.TaskFailed: ".len() + 300);

        let none = stage_error(Some("States.TaskFailed"), None);
        assert_eq!(none, "States.TaskFailed: no cause recorded");
        let keyless = stage_error(Some("States.TaskFailed"), Some(r#"{"RoleArn":"x"}"#));
        assert_eq!(keyless, "States.TaskFailed: no cause recorded");

        assert_eq!(stage_error(Some(""), None), "error: no cause recorded");
        assert_eq!(stage_error(None, None), "error: no cause recorded");
        let hostile_name = format!("arn:aws:iam::{ACCOUNT}:role/r");
        assert_eq!(
            stage_error(Some(&hostile_name), None),
            "error: no cause recorded"
        );
        assert_eq!(
            stage_error(Some(&"E".repeat(65)), None),
            "error: no cause recorded"
        );
        assert_eq!(
            stage_error(Some("AKIAIOSFODNN7EXAMPLE"), None),
            "[redacted] no cause recorded"
        );

        for text in [sm, ecs, plain, long] {
            assert_clean(&text);
            assert!(text.chars().count() <= MAX_CALLER_TEXT_CHARS);
            assert_eq!(caller_safe(&text, MAX_CALLER_TEXT_CHARS), text);
        }
    }

    #[test]
    fn stage_error_keeps_only_the_first_four_reasons() {
        let cause = serde_json::json!({
            "FailureReason": "a", "Reason": "b", "SecondaryStatus": "c",
            "StopCode": "d", "TrainingJobStatus": "e", "errorMessage": "f",
        })
        .to_string();
        assert_eq!(stage_error(Some("E"), Some(&cause)), "E: a; b; c; d");
    }
}
