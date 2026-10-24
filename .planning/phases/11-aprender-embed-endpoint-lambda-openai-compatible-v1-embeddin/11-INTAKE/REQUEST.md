# Requester intake request: aprender-embed endpoint (phase 11, plan 11-03)

For: the pmcp.run graph-rag team. Send the six items below. Partial delivery is fine.
`pending` is a valid reply: it means nothing has arrived yet, and the plan completes with every item marked missing.

## Send sanitized copies on the first send

The intake scans every file it commits for: 12-digit sequences (account ids), `AKIA` or `ASIA`
access-key prefixes, `Bearer ` tokens, and `x-amz-security-token` header values. A 12-digit
example value makes the scan refuse the file. So replace account ids, ARNs, session tokens,
access keys, bearer tokens and source addresses with placeholders before you send anything.
If a file contains any of these, send a sanitized copy instead of the original.

## The six items

1. **The brief.** The conformance checks, as you wrote them, verbatim. Save it as `11-BRIEF.md`
   (the executor adds a header with the source and capture date; send the body untouched).
   Used for: the thirteen conformance checks, which are the acceptance target.
   Possible source on your side, to confirm and not assumed: `built-in/graph-rag/.planning/phases/205-localized-embeddings-bedrock-titan-v2-and-in-account-aprende/205-APRENDER-EMBED-HANDOFF.md`.
   If that is not the brief you intend, send the brief you intend.

2. **The tool `aprender-embed-conformance`.** Where it lives (repository, path and commit), how it is
   run, and the exact line it prints on success. Possible location on your side, to confirm:
   the `aprender-embed-conformance base_url reference:` recipe in `built-in/graph-rag/justfile`, and
   `built-in/graph-rag/crates/snapshot-build/tests/aprender_conformance.rs`.
   Either send the check-definition files (up to 200 KB in total) verbatim, saved into
   `11-INTAKE/` under their own file names, or point at the checkout path and commit.
   Used for: the local conformance harness (plan 11-11) and the pass line.

3. **The schema of `aprender-embed-reference-vectors.json`.** Field names and types, as a sanitized
   sample file, a spec, or the tool's loader. Save it into `11-INTAKE/` under its own file name.
   Used for: the shape of the reference file the endpoint's vectors are checked against.

4. **A captured platform event.** The exact JSON that the platform or the tool sends to the function
   for `POST /v1/embeddings`, sanitized as in item 0 above. Save it as
   `deploy/embed-endpoint/events/apigw-v2-platform-captured.json`.
   Also tell us who builds that event: your tool, or pmcp.run. If no such event can exist, say so.
   Then the endpoint is built to the documented default shape,
   `deploy/llm-endpoint/events/apigw-v2-no-auth.json`.

5. **The function-name allowlist answer.** Does the platform invoke only function names with a required
   prefix? The 2026-10-18 change request documented `aprender-llm-` and `pmcp-llm-`.
   If yes, what name must the function carry? The plan's default is `aprender-embed-endpoint-<stage>`.
   Reply with the answer verbatim and save it in `11-INTAKE/answers.md`.

6. **The envelope-policy answers.** Save them verbatim in `11-INTAKE/answers.md`, one line per item.
   Each may be answered `unspecified`.
   a. What `usage.prompt_tokens` counts: consumed positions after truncation, special tokens included,
      or the original length.
   b. Whether an empty string or an empty array is refused, or embedded.
   c. Whether integer-token inputs are in scope (this encoder has no token-id input).
   d. The accepted `dimensions` values.
   e. Whether unknown request fields are ignored or refused.
   f. What, if anything, should produce a 429 from inside the function. Lambda's own throttling reaches
      a caller as TooManyRequestsException, not as a function response.
   g. The largest number of inputs you will send per call, and the typical input size.
   h. The model id strings you use (aliases included).
   i. Whether extra response headers are acceptable.
   j. Whether the caller is in the same AWS account: yes or no, and the principal kind (an IAM role,
      a user, or an account). Never send an ARN or an id.
   k. The stage names and the region.

## How to reply

- `intake-ready` followed by where the material is (a path, or `pasted below`)
- `partial: <items still missing>`
- `pending` (nothing has arrived yet; the plan is re-run when material arrives)
- `blocked: <reason>`
