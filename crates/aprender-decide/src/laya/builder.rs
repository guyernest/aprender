//! Port of Laya's request builder, `laya.common.build_sequence` (string state, no
//! option reordering, right truncation), with the D-12 truncation flag.
//!
//! ```text
//! [CLS] "{t} question: {ins}" [SEP] [MASK] opt0 [MASK] opt1 ... [SEP] state [SEP]
//! ```
//!
//! - the literal mask token in `ins`, every option and the state is replaced by a
//!   space; `[SEP]` / `[CLS]` typed by a caller are NOT, and tokenize to their special
//!   ids exactly as they do in Laya (RESEARCH Pitfall 10: reproduced, not "fixed").
//!   Markers come from this builder, never from scanning ids, so injected text cannot
//!   add an option;
//! - each option is `[MASK]` + the first 48 tokens of `" " + option`;
//! - when the options leave fewer than 16 tokens of `head_max_len`, each option is cut
//!   to `max(4, (head_max_len - 16) / K)` tokens (the SHRINK — served, not refused);
//! - the head keeps at least 8 tokens;
//! - the state fills the room left before `max_len` (right truncation), the row is
//!   capped at `max_len`, and only markers below `max_len` survive.
//!
//! Marker loss (fewer surviving markers than options) is not decided here: the caller
//! refuses it once, at task load (`LayaError::MarkersLost`).

use super::LayaError;
use tokenizers::Tokenizer;

/// Option text is capped at this many tokens before the shrink (Laya `max_length=48`).
const OPTION_MAX_TOKENS: usize = 48;
/// The shrink triggers when the options leave fewer than this many head tokens.
const MIN_OPTION_BUDGET: isize = 16;
/// Each option keeps at least this many tokens (its marker included) when shrunk.
const MIN_SHRUNK_OPTION: isize = 4;
/// The head (`"{t} question: {ins}"`) keeps at least this many tokens.
const MIN_HEAD_TOKENS: isize = 8;

/// One built row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltRow {
    /// Token ids, at most `max_len`.
    pub ids: Vec<u32>,
    /// Position of each surviving option's `[MASK]` marker, in option order.
    pub markers: Vec<usize>,
    /// `ids.len()`.
    pub tokens: usize,
    /// True exactly when the untruncated row would exceed `max_len` — i.e. the state
    /// had more tokens than the room left for it (D-12).
    pub truncated: bool,
}

/// Laya's request builder over a byte-identical `tokenizer.json`.
pub struct Builder {
    tok: Tokenizer,
    cls: u32,
    sep: u32,
    mask: u32,
    mask_token: String,
    max_len: usize,
    head_max_len: usize,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder")
            .field("cls", &self.cls)
            .field("sep", &self.sep)
            .field("mask", &self.mask)
            .field("max_len", &self.max_len)
            .field("head_max_len", &self.head_max_len)
            .finish_non_exhaustive()
    }
}

impl Builder {
    /// Build from the raw `tokenizer.json` bytes and the agent config's lengths.
    ///
    /// # Errors
    ///
    /// [`LayaError::Tokenizer`] when the bytes are not a tokenizer, and
    /// [`LayaError::MissingSpecialToken`] when `[CLS]`, `[SEP]` or `[MASK]` is absent.
    pub fn from_bytes(
        tokenizer_bytes: &[u8],
        max_len: usize,
        head_max_len: usize,
    ) -> Result<Self, LayaError> {
        let tok = Tokenizer::from_bytes(tokenizer_bytes)
            .map_err(|e| LayaError::Tokenizer(e.to_string()))?;
        let id = |t: &str| {
            tok.token_to_id(t)
                .ok_or_else(|| LayaError::MissingSpecialToken(t.to_string()))
        };
        let (cls, sep, mask) = (id("[CLS]")?, id("[SEP]")?, id("[MASK]")?);
        Ok(Self {
            tok,
            cls,
            sep,
            mask,
            mask_token: "[MASK]".to_string(),
            max_len,
            head_max_len,
        })
    }

    /// The row cap (`max_len`).
    #[must_use]
    pub fn max_len(&self) -> usize {
        self.max_len
    }

    /// The head + options budget (`head_max_len`).
    #[must_use]
    pub fn head_max_len(&self) -> usize {
        self.head_max_len
    }

    /// Tokenize without special tokens (Laya `add_special_tokens=False`).
    fn enc(&self, s: &str) -> Result<Vec<u32>, LayaError> {
        Ok(self
            .tok
            .encode(s, false)
            .map_err(|e| LayaError::Tokenizer(e.to_string()))?
            .get_ids()
            .to_vec())
    }

    /// Build the row for `state` against question type `t`, instructions `ins` and the
    /// rendered `options` (label order).
    ///
    /// # Errors
    ///
    /// [`LayaError::Tokenizer`] when the tokenizer refuses a text.
    #[provable_contracts_macros::contract("laya-parity-v1", equation = "ids_exact")]
    pub fn build(
        &self,
        state: &str,
        t: &str,
        ins: &str,
        options: &[String],
    ) -> Result<BuiltRow, LayaError> {
        let mt = self.mask_token.as_str();
        let ins = ins.replace(mt, " ");
        let head_ids = self.enc(&format!("{t} question: {ins}"))?;
        let mut opt_ids = options
            .iter()
            .map(|o| {
                let mut v = vec![self.mask];
                v.extend(
                    self.enc(&format!(" {}", o.replace(mt, " ")))?
                        .into_iter()
                        .take(OPTION_MAX_TOKENS),
                );
                Ok(v)
            })
            .collect::<Result<Vec<Vec<u32>>, LayaError>>()?;
        let hm = to_isize(self.head_max_len);
        let opt_total = |o: &[Vec<u32>]| o.iter().map(|x| to_isize(x.len())).sum::<isize>();
        let mut budget = hm - opt_total(&opt_ids);
        if budget < MIN_OPTION_BUDGET {
            let k = to_isize(opt_ids.len().max(1));
            let per =
                usize::try_from(MIN_SHRUNK_OPTION.max((hm - MIN_OPTION_BUDGET).div_euclid(k)))
                    .unwrap_or(usize::MAX);
            for o in &mut opt_ids {
                o.truncate(per);
            }
            budget = hm - opt_total(&opt_ids);
        }
        let keep = usize::try_from(MIN_HEAD_TOKENS.max(budget)).unwrap_or(usize::MAX);
        let mut ids = vec![self.cls];
        ids.extend(head_ids.into_iter().take(keep));
        ids.push(self.sep);
        let mut markers = Vec::with_capacity(opt_ids.len());
        for o in opt_ids {
            markers.push(ids.len());
            ids.extend(o);
        }
        ids.push(self.sep);
        let room = self.max_len.saturating_sub(ids.len() + 1);
        let state_ids = self.enc(&state.replace(mt, " "))?;
        // The untruncated row is prefix + state + the closing [SEP].
        let truncated = ids.len() + state_ids.len() + 1 > self.max_len;
        ids.extend(state_ids.into_iter().take(room));
        ids.push(self.sep);
        ids.truncate(self.max_len);
        markers.retain(|&m| m < self.max_len);
        Ok(BuiltRow {
            tokens: ids.len(),
            ids,
            markers,
            truncated,
        })
    }
}

/// Token counts are bounded by the tokenizer's output length; saturate rather than wrap.
fn to_isize(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}
