# TweetEval abortion stance benchmark

This benchmark is aprender's reference social-media classification dataset for
comparing lightweight sentence-encoder classifiers, SetFit-style contrastive
fine-tuning, and larger LoRA classifiers.

The default command downloads a pinned revision of canonical TweetEval and
converts it to aprender JSONL. The original tweet text is not stored in this
repository.

```bash
apr data tweet-eval-stance --output data/tweet-eval-stance
```

The generated canonical layout is:

```text
data/tweet-eval-stance/
├── train.jsonl               # 587 samples
├── validation.jsonl          # 66 samples
├── test.jsonl                # 280 samples
└── benchmark-manifest.json   # labels, hashes, provenance, protocol
```

Each JSONL row can be loaded directly by the classification fine-tuning
pipeline:

```json
{"id":"train:0","input":"...","label":1,"label_text":"against","source_split":"train"}
```

The fixed label mapping is `0=none`, `1=against`, and `2=favor`.

## Offline or mirrored preparation

Supply a directory containing the canonical `train_text.txt`,
`train_labels.txt`, `val_text.txt`, `val_labels.txt`, `test_text.txt`, and
`test_labels.txt` files:

```bash
apr --offline data tweet-eval-stance \
  --source /datasets/tweeteval/datasets/stance/abortion \
  --output data/tweet-eval-stance
```

The command verifies the exact canonical split and class counts before writing
anything. Existing benchmark outputs are not replaced unless `--force` is
provided.

When files come from `--source`, the `--revision` recorded in the manifest is
your assertion, not a verified fact — the command cannot prove a local
directory came from that commit. The manifest records this explicitly as
`source.revision_verified: false`; it is `true` only for runs that downloaded
the pinned revision themselves.

## SetFit compatibility layout

For comparison with the two-split SetFit Hugging Face wrapper:

```bash
apr data tweet-eval-stance \
  --profile setfit \
  --output data/tweet-eval-stance-setfit
```

This emits 587 training rows and a 346-row test file made by concatenating the
canonical validation and test sets. It is a compatibility mode, not the
recommended model-selection protocol.

## 9B classifier baseline

Use an actual pretrained model artifact rather than `--model-size` alone:

```bash
apr finetune qwen-9b.apr \
  --task classify \
  --num-classes 3 \
  --data data/tweet-eval-stance/train.jsonl \
  --output checkpoints/tweet-eval-qwen-9b
```

Select checkpoints using `validation.jsonl`, then evaluate the fixed test set:

```bash
apr eval checkpoints/tweet-eval-qwen-9b \
  --task classify \
  --dataset tweet-eval-stance \
  --num-classes 3 \
  --model-size 9B \
  --data data/tweet-eval-stance/test.jsonl
```

The benchmark's primary metric is the official stance score:

```text
F_avg = (F1_against + F1_favor) / 2
```

The evaluation report also includes three-class macro-F1, per-class metrics,
MCC, calibration, confidence intervals, and confusion diagnostics. Accuracy is
supplemental because the test split is dominated by the `against` class.

For few-shot comparisons, the manifest fixes balanced 8, 16, 32, and 64
examples per class and ten deterministic seeds. All classifier variants must
use the same sampled IDs and must not use the canonical test split for tuning.

The normal test suite uses authored fixtures and makes no network requests. To
check the pinned upstream files explicitly, run the ignored integration test:

```bash
cargo test -p apr-cli pinned_upstream_satisfies_the_dataset_contract -- --ignored
```
