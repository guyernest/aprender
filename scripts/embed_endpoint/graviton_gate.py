#!/usr/bin/env python3
"""Graviton gate: REPORT parser and the embed-graviton-v1 normalizer (Phase 11, plan 11-01).

usage:
  graviton_gate.py normalize <raw.json> (--out <file> | --stdout)
  graviton_gate.py selftest

parse_report(text) reads the last REPORT line of a Lambda log tail. proven_cold is
true exactly when the REPORT line carries an Init Duration.

normalize(raw) turns the raw record that graviton_run.py writes into the evidence
shape embed-graviton-v1. Every derived number is computed here, by this code, from
the samples. Plans 11-04 and 11-08 read the key names below BY NAME, so they are
asserted in the selftest and must not drift:

  schema, outcome, failed_checks, reported_not_promised, function, ingress,
  cpu_parts, samples, derived (warm_64_encode_ms_median, warm_64_encode_ms_max,
  cold_64_duration_ms_max, measured_max_inputs, invoke_attempts_total,
  billed_ms_total, warm_ms_per_input_256_max, cold_1_client_wall_ms_max,
  canary_within_30s, batch64_within_30s_warm, batch64_within_30s_cold,
  tokenize_ms_per_mib_max, max_memory_used_mb_max, research_bracket_s, vs_bracket)

The outcome is `measured` only when every check holds (see normalize). A run that
fails any check is `failed`, with the checks named. The derived numbers are REPORTED,
never promised: reported_not_promised is always true.

Standard library only. It imports no sibling module and makes no AWS call.
"""

import json
import re
import statistics
import sys

SCHEMA = "embed-graviton-v1"
RAW_SCHEMA = "embed-graviton-raw-v1"
EMBED_KINDS = ("cold-1", "warm-1", "cold-64", "warm-64")
TOKENIZE_KIND = "tokenize-1mib"
SIXTY_FOUR_KINDS = ("cold-64", "warm-64")
WARM_KINDS = ("warm-1", "warm-64", TOKENIZE_KIND)
DIM = 384
MIB = 1048576
THIRTY_S_MS = 30000
RESEARCH_BRACKET_S = [5.3, 50]
TOP_KEYS = {
    "schema", "outcome", "failed_checks", "reported_not_promised", "function",
    "ingress", "cpu_parts", "samples", "derived",
}
DERIVED_KEYS = {
    "warm_64_encode_ms_median", "warm_64_encode_ms_max", "cold_64_duration_ms_max",
    "measured_max_inputs", "invoke_attempts_total", "billed_ms_total",
    "warm_ms_per_input_256_max", "cold_1_client_wall_ms_max", "canary_within_30s",
    "batch64_within_30s_warm", "batch64_within_30s_cold", "tokenize_ms_per_mib_max",
    "max_memory_used_mb_max", "research_bracket_s", "vs_bracket",
}
TWELVE_DIGITS = re.compile(r"\b\d{12}\b")

REPORT_RE = re.compile(
    r"REPORT RequestId: \S+\s+Duration: ([\d.]+) ms\s+Billed Duration: (\d+) ms\s+"
    r"Memory Size: (\d+) MB\s+Max Memory Used: (\d+) MB"
    r"(?:\s+Init Duration: ([\d.]+) ms)?"
)


def parse_report(text):
    """The last REPORT line of a log tail as a dict, or None when there is none."""
    matches = REPORT_RE.findall(text or "")
    if not matches:
        return None
    duration, billed, memory, max_used, init = matches[-1]
    init_ms = float(init) if init else None
    return {
        "duration_ms": float(duration),
        "billed_ms": int(billed),
        "memory_size_mb": int(memory),
        "max_memory_used_mb": int(max_used),
        "init_duration_ms": init_ms,
        "proven_cold": init_ms is not None,
    }


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def response_ok(kind, event, probe):
    """Validity per kind, and by no other rule.

    Embedding kinds: no `error` key, rows equal to the event's n, dim 384.
    tokenize-1mib encodes nothing, so it has no rows or dim: no `error` key, a
    numeric tokenize_bytes_ms, and a tokenize_kind equal to the one requested.
    """
    if not isinstance(probe, dict) or "error" in probe:
        return False
    if kind in EMBED_KINDS:
        return probe.get("rows") == event.get("n", 64) and probe.get("dim") == DIM
    if kind == TOKENIZE_KIND:
        return (
            is_number(probe.get("tokenize_bytes_ms"))
            and probe.get("tokenize_kind") == event.get("tokenize_kind", "prose")
        )
    return False


def flatten(sample):
    report = sample.get("report") or {}
    init = report.get("init_duration_ms")
    return {
        "kind": sample["kind"],
        "proven_cold": init is not None,
        "response_ok": response_ok(sample["kind"], sample.get("event") or {}, sample.get("probe")),
        "attempts": int(sample.get("attempts", 1)),
        "client_wall_ms": sample.get("client_wall_ms"),
        "duration_ms": report.get("duration_ms"),
        "billed_ms": report.get("billed_ms"),
        "init_duration_ms": init,
        "max_memory_used_mb": report.get("max_memory_used_mb"),
        "function_error": sample.get("function_error"),
        "probe": sample.get("probe"),
    }


def _round3(value):
    if isinstance(value, float):
        return round(value, 3)
    if isinstance(value, list):
        return [_round3(item) for item in value]
    if isinstance(value, dict):
        return {key: _round3(item) for key, item in value.items()}
    return value


def _max_or_none(values):
    values = [v for v in values if v is not None]
    return max(values) if values else None


def normalize(raw):
    """The embed-graviton-v1 evidence for one raw record. Raises ValueError on a 12-digit run."""
    if TWELVE_DIGITS.search(json.dumps(raw, sort_keys=True)):
        raise ValueError("refusing to normalize a raw record that contains a 12-digit sequence")
    samples = raw.get("samples", [])
    function = dict(raw.get("function") or {})
    ingress = raw.get("ingress") or {}
    views = [flatten(s) for s in samples]
    failed = []

    def fail(check):
        if check not in failed:
            failed.append(check)

    if any(v["function_error"] for v in views):
        fail("function_error")
    for v in views:
        kind = v["kind"]
        probe = v["probe"] if isinstance(v["probe"], dict) else {}
        if v["duration_ms"] is None:
            fail(f"report missing: {kind}")
        if not v["response_ok"]:
            fail(f"response_ok: {kind}")
        if kind in WARM_KINDS and v["proven_cold"]:
            fail(f"warm sample carries an Init Duration: {kind}")
        if kind in SIXTY_FOUR_KINDS:
            if probe.get("truncated_count") != probe.get("n") or probe.get("tape_len_after") != 0:
                fail(f"64-input sample truncation or tape check: {kind}")
    one_cold = [v for v in views if v["kind"] == "cold-1" and v["proven_cold"]
                and v["response_ok"] and not v["function_error"]]
    if len(one_cold) < 2:
        fail("fewer than two proven-cold one-input samples")
    if not any(v["kind"] == "cold-64" and v["proven_cold"] and v["response_ok"] for v in views):
        fail("no proven-cold 64-input sample")
    if function.get("code_sha256") != function.get("expected_code_sha256"):
        fail("code_sha256 differs from expected_code_sha256")
    for key in ("function_url", "policy"):
        if ingress.get(key) != "absent":
            fail(f"ingress {key} is not absent")

    def valid64(v):
        probe = v["probe"] if isinstance(v["probe"], dict) else {}
        return (v["kind"] in SIXTY_FOUR_KINDS and v["response_ok"] and not v["function_error"]
                and probe.get("truncated_count") == probe.get("n") and probe.get("tape_len_after") == 0)

    warm64 = [v for v in views if v["kind"] == "warm-64" and v["response_ok"] and not v["function_error"]]
    cold64 = [v for v in views if v["kind"] == "cold-64" and v["proven_cold"]
              and v["response_ok"] and not v["function_error"]]
    tokens = [v for v in views if v["kind"] == TOKENIZE_KIND and v["response_ok"]]
    warm64_encode = [v["probe"]["encode_ms"] for v in warm64 if is_number(v["probe"].get("encode_ms"))]
    per_input = [v["probe"]["encode_ms"] / v["probe"]["n"] for v in warm64
                 if is_number(v["probe"].get("encode_ms")) and v["probe"].get("n")]
    cold64_max = _max_or_none([v["duration_ms"] for v in cold64])
    cold64_seconds = None if cold64_max is None else cold64_max / 1000.0
    if cold64_seconds is None:
        vs_bracket = None
    elif cold64_seconds < RESEARCH_BRACKET_S[0]:
        vs_bracket = "below"
    elif cold64_seconds > RESEARCH_BRACKET_S[1]:
        vs_bracket = "above"
    else:
        vs_bracket = "within"
    cold1_walls = [v["client_wall_ms"] for v in one_cold if is_number(v["client_wall_ms"])]
    cold1_max = _max_or_none(cold1_walls)
    warm64_durations = [v["duration_ms"] for v in warm64 if is_number(v["duration_ms"])]
    tokenize_scaled = [
        v["probe"]["tokenize_bytes_ms"] * MIB / (v["probe"].get("tokenize_bytes") or 0)
        for v in tokens if (v["probe"].get("tokenize_bytes") or 0) > 0
    ]
    billed_total = 0
    for sample in samples:
        billed_total += (sample.get("report") or {}).get("billed_ms") or 0
        for attempt in sample.get("discarded_attempts") or []:
            billed_total += (attempt.get("report") or {}).get("billed_ms") or 0

    cpu_parts = sorted({v["probe"]["cpu_part"] for v in views
                        if isinstance(v["probe"], dict) and v["probe"].get("cpu_part")})
    valid_inputs = [v["probe"].get("rows") for v in views if valid64(v)]
    outcome = "failed" if failed else "measured"

    evidence = {
        "schema": SCHEMA,
        "outcome": outcome,
        "failed_checks": failed,
        "reported_not_promised": True,
        "function": function,
        "ingress": {"function_url": ingress.get("function_url"), "policy": ingress.get("policy")},
        "cpu_parts": cpu_parts,
        "samples": [
            {key: v[key] for key in ("kind", "proven_cold", "response_ok", "attempts",
                                     "client_wall_ms", "duration_ms", "billed_ms",
                                     "init_duration_ms", "max_memory_used_mb",
                                     "function_error", "probe")}
            for v in views
        ],
        "derived": {
            "warm_64_encode_ms_median": statistics.median(warm64_encode) if warm64_encode else None,
            "warm_64_encode_ms_max": _max_or_none(warm64_encode),
            "cold_64_duration_ms_max": cold64_max,
            "measured_max_inputs": _max_or_none(valid_inputs),
            "invoke_attempts_total": sum(v["attempts"] for v in views),
            "billed_ms_total": billed_total,
            "warm_ms_per_input_256_max": _max_or_none(per_input),
            "cold_1_client_wall_ms_max": cold1_max,
            "canary_within_30s": cold1_max is not None and cold1_max < THIRTY_S_MS,
            "batch64_within_30s_warm": bool(warm64_durations) and max(warm64_durations) < THIRTY_S_MS,
            "batch64_within_30s_cold": cold64_max is not None and cold64_max < THIRTY_S_MS,
            "tokenize_ms_per_mib_max": _max_or_none(tokenize_scaled),
            "max_memory_used_mb_max": _max_or_none([v["max_memory_used_mb"] for v in views]),
            "research_bracket_s": list(RESEARCH_BRACKET_S),
            "vs_bracket": vs_bracket,
        },
    }
    return _round3(evidence)


def evidence_text(evidence):
    """The JSON text to write. Refuses any 12-digit sequence (account id guard)."""
    text = json.dumps(evidence, indent=2, sort_keys=True) + "\n"
    if TWELVE_DIGITS.search(text):
        raise ValueError("refusing to write evidence that contains a 12-digit sequence")
    return text


# ---------------------------------------------------------------------------
# Selftest: a case table over the parser and the normalizer.
# ---------------------------------------------------------------------------

PROBE_NAME = "aprender-embed-graviton-probe"
EV1 = {"n": 1, "micro_batch": 8, "words": 700}
EV64 = {"n": 64, "micro_batch": 8, "words": 700}
EVTOK = {"tokenize_bytes": 1048576, "tokenize_kind": "prose"}
EVTOK_ADV = {"tokenize_bytes": 1048576, "tokenize_kind": "adversarial"}
REPORT_COLD = ("REPORT RequestId: 0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\tDuration: 1234.56 ms\t"
               "Billed Duration: 1235 ms\tMemory Size: 10240 MB\tMax Memory Used: 3500 MB\t"
               "Init Duration: 512.34 ms\n")
REPORT_WARM = ("REPORT RequestId: 1b2c3d4e-5f60-4a7b-8c9d-0e1f2a3b4c5d\tDuration: 980.1 ms\t"
               "Billed Duration: 981 ms\tMemory Size: 10240 MB\tMax Memory Used: 3400 MB\n")


def good_probe(n, cpu="0xd40", encode_ms=900.0):
    return {"probe": PROBE_NAME, "n": n, "rows": n, "dim": DIM, "truncated_count": n,
            "consumed_positions": n * 256, "tape_len_after": 0, "encode_ms": encode_ms,
            "cpu_part": cpu, "rayon_pool_threads": 6}


def tok_probe(kind="prose", ms=600.0, size=1048576):
    return {"probe": PROBE_NAME, "tokenize_bytes_ms": ms, "tokenize_kind": kind,
            "tokenize_bytes": size}


def sample(kind, event, *, init=None, duration=900.0, billed=1000, wall=1000.0, probe=None,
           function_error=None, attempts=1, discarded=None):
    return {
        "kind": kind,
        "event": event,
        "function_error": function_error,
        "client_wall_ms": wall,
        "attempts": attempts,
        "report": {"duration_ms": duration, "billed_ms": billed, "memory_size_mb": 10240,
                   "max_memory_used_mb": 3500, "init_duration_ms": init},
        "probe": probe,
        "discarded_attempts": discarded or [],
    }


def raw_run(samples, code="sha-live", expected="sha-live"):
    return {
        "schema": RAW_SCHEMA,
        "function": {"name": PROBE_NAME, "region": "us-east-1", "memory_mb": 10240,
                     "timeout_s": 120, "architecture": "arm64", "rayon_threads_env": "6",
                     "code_sha256": code, "expected_code_sha256": expected},
        "ingress": {"function_url": "absent", "policy": "absent"},
        "samples": samples,
    }


def good_samples():
    return (
        [sample("cold-1", EV1, init=280.0, wall=1200.0, probe=good_probe(1)) for _ in range(3)]
        + [sample("warm-1", EV1, wall=300.0, probe=good_probe(1)) for _ in range(5)]
        + [sample("cold-64", EV64, init=300.0, duration=21000.0, wall=21500.0, probe=good_probe(64))
           for _ in range(2)]
        + [sample("warm-64", EV64, duration=20000.0, wall=20100.0,
                  probe=good_probe(64, encode_ms=19000.0)) for _ in range(3)]
        + [sample(TOKENIZE_KIND, EVTOK, probe=tok_probe("prose")),
           sample(TOKENIZE_KIND, EVTOK_ADV, probe=tok_probe("adversarial"))]
    )


def selftest():
    cases = 0

    def expect(condition, name):
        nonlocal cases
        if not condition:
            print(f"GRAVITON GATE SELFTEST FAILED: {name}", file=sys.stderr)
            sys.exit(1)
        cases += 1

    def outcome_of(samples, **kwargs):
        return normalize(raw_run(samples, **kwargs))

    # The REPORT parser.
    cold = parse_report(REPORT_COLD)
    expect(cold is not None and cold["proven_cold"] is True and cold["init_duration_ms"] == 512.34,
           "a REPORT line with Init Duration is proven cold")
    warm = parse_report(REPORT_WARM)
    expect(warm is not None and warm["proven_cold"] is False and warm["init_duration_ms"] is None,
           "a REPORT line without Init Duration is not proven cold")
    expect(parse_report("START RequestId: x\nEND RequestId: x\n") is None,
           "a log tail with no REPORT line parses to None")
    expect(parse_report(REPORT_COLD + REPORT_WARM)["proven_cold"] is False,
           "the last REPORT line of a tail is the one that counts")

    # The good run and the timing derivations.
    good = outcome_of(good_samples())
    expect(good["outcome"] == "measured" and good["failed_checks"] == [],
           "a correct run, with tokenize samples, is measured")
    expect(good["derived"]["batch64_within_30s_warm"] is True, "warm 64-input at 20 s is within 30 s")
    for durations, want, name in (
        ([5100.0, 29900.0, 29900.0], True, "a warm 64-input set on both sides of 5.1 s and 29.9 s"),
        ([29900.0, 30100.0], False, "a warm 64-input sample at 30.1 s is above 30 s"),
    ):
        samples = good_samples()
        for index, duration in enumerate(durations):
            samples[10 + index]["report"]["duration_ms"] = duration
        expect(outcome_of(samples)["derived"]["batch64_within_30s_warm"] is want, name)
    for duration, want in ((29900.0, True), (30100.0, False)):
        samples = good_samples()
        samples[8]["report"]["duration_ms"] = duration
        samples[9]["report"]["duration_ms"] = duration
        expect(outcome_of(samples)["derived"]["batch64_within_30s_cold"] is want,
               f"cold 64-input at {duration} ms gives batch64_within_30s_cold {want}")
    for wall, want in ((29900.0, True), (30100.0, False)):
        samples = good_samples()
        for index in (0, 1, 2):
            samples[index]["client_wall_ms"] = wall
        expect(outcome_of(samples)["derived"]["canary_within_30s"] is want,
               f"a cold one-input client wall of {wall} ms gives canary_within_30s {want}")

    # Refusals: each one turns the run to failed and names the check.
    def failed_with(samples, text, **kwargs):
        result = outcome_of(samples, **kwargs)
        return result["outcome"] == "failed" and any(text in check for check in result["failed_checks"])

    samples = good_samples()
    samples[7]["function_error"] = "Unhandled"
    expect(failed_with(samples, "function_error"), "a sample with a FunctionError is failed")

    samples = good_samples()
    samples[12]["probe"]["truncated_count"] = 60
    expect(failed_with(samples, "64-input"), "a 64-input sample with truncated_count below n is failed")

    samples = good_samples()
    for index in (0, 1, 2):
        samples[index]["report"]["init_duration_ms"] = None
    expect(failed_with(samples, "fewer than two proven-cold"), "no proven-cold one-input sample is failed")

    samples = good_samples()
    samples[4]["report"]["init_duration_ms"] = 120.0
    expect(failed_with(samples, "warm sample carries"), "a warm sample with an Init Duration is failed")

    samples = good_samples()
    samples[0]["probe"] = {"probe": PROBE_NAME, "error": "encode failed"}
    expect(failed_with(samples, "response_ok"), "a probe response with an error key is failed")

    samples = good_samples()
    samples[8]["probe"]["rows"] = 60
    expect(failed_with(samples, "response_ok: cold-64"),
           "a 64-input response with too few rows is failed")

    result = outcome_of(good_samples(), code="sha-other", expected="sha-live")
    expect(result["outcome"] == "failed" and "code_sha256 differs from expected_code_sha256"
           in result["failed_checks"], "an expected_code_sha256 that differs from the live code is failed")

    # measured_max_inputs.
    expect(good["derived"]["measured_max_inputs"] == 64, "the largest valid 64-class sample sets measured_max_inputs")
    samples = good_samples()
    for index in range(8, 13):
        if samples[index]["kind"] in SIXTY_FOUR_KINDS:
            samples[index]["probe"]["truncated_count"] = 1
    expect(outcome_of(samples)["derived"]["measured_max_inputs"] is None,
           "no valid 64-class sample leaves measured_max_inputs null")

    # Tokenize samples: valid by their own rule, with no rows or dim.
    tokens = [s for s in good["samples"] if s["kind"] == TOKENIZE_KIND]
    expect(len(tokens) == 2 and all(t["response_ok"] for t in tokens),
           "correct tokenize samples are response_ok without rows or dim")
    samples = good_samples()
    samples[-1]["probe"] = {"probe": PROBE_NAME, "error": "tokenize failed"}
    expect(failed_with(samples, "response_ok: tokenize-1mib"), "a tokenize sample with an error key is failed")
    samples = good_samples()
    del samples[-1]["probe"]["tokenize_bytes_ms"]
    expect(failed_with(samples, "response_ok: tokenize-1mib"), "a tokenize sample without tokenize_bytes_ms is failed")
    samples = good_samples()
    samples[-1]["probe"]["tokenize_kind"] = "prose"
    expect(failed_with(samples, "response_ok: tokenize-1mib"),
           "a tokenize sample whose kind differs from the event's is failed")
    samples = good_samples()
    del samples[0]["probe"]["rows"]
    expect(failed_with(samples, "response_ok: cold-1"), "an embedding sample with no rows is response_ok false")

    # Derived numbers.
    samples = good_samples()
    samples[-2]["probe"]["tokenize_bytes_ms"] = 40.0
    samples[-2]["probe"]["tokenize_bytes"] = 524288
    samples[-1]["probe"]["tokenize_bytes_ms"] = 40.0
    samples[-1]["probe"]["tokenize_bytes"] = 524288
    expect(outcome_of(samples)["derived"]["tokenize_ms_per_mib_max"] == 80.0,
           "a half-MiB tokenize sample is scaled to one MiB")
    expect(good["derived"]["tokenize_ms_per_mib_max"] == 600.0, "the 1 MiB tokenize sample is reported as is")
    expect(good["derived"]["warm_ms_per_input_256_max"] == round(19000.0 / 64, 3),
           "warm encode is divided by n for the per-input figure")
    expect(good["derived"]["warm_64_encode_ms_median"] == 19000.0, "the warm 64 encode median is taken")
    samples = good_samples()
    samples[10]["discarded_attempts"] = [{"report": {"billed_ms": 777}, "function_error": None}]
    samples[10]["attempts"] = 2
    result = outcome_of(samples)
    expect(result["derived"]["invoke_attempts_total"] == 16
           and result["derived"]["billed_ms_total"] == good["derived"]["billed_ms_total"] + 777,
           "discarded attempts count toward attempts and billed milliseconds")
    expect(good["derived"]["invoke_attempts_total"] == 15 and
           good["derived"]["billed_ms_total"] == 15 * 1000, "the nominal run counts 15 attempts")
    expect(good["derived"]["cold_64_duration_ms_max"] == 21000.0, "the largest cold 64-input duration")
    expect(good["derived"]["cold_1_client_wall_ms_max"] == 1200.0, "the largest cold one-input client wall")
    expect(good["derived"]["max_memory_used_mb_max"] == 3500, "the largest memory use")
    for duration, want in ((4000.0, "below"), (21000.0, "within"), (60000.0, "above")):
        samples = good_samples()
        for index in (8, 9):
            samples[index]["report"]["duration_ms"] = duration
        expect(outcome_of(samples)["derived"]["vs_bracket"] == want,
               f"a cold 64-input duration of {duration} ms is {want} the research bracket")
    expect(good["derived"]["research_bracket_s"] == [5.3, 50], "the research bracket is carried as context")
    expect(good["cpu_parts"] == ["0xd40"], "cpu_parts is the sorted distinct set")
    samples = good_samples()
    samples[0]["probe"]["cpu_part"] = "0xd0c"
    expect(outcome_of(samples)["cpu_parts"] == ["0xd0c", "0xd40"], "two cpu parts are sorted and distinct")

    # Output key names, asserted exactly, because plans 11-04 and 11-08 read them by name.
    expect(set(good) == TOP_KEYS, f"top-level evidence keys are exact: {sorted(good)}")
    expect(set(good["derived"]) == DERIVED_KEYS, "every derived key name is exact")
    expect({"code_sha256", "expected_code_sha256", "memory_mb", "name"} <= set(good["function"]),
           "the function object carries the live and the expected CodeSha256")
    expect(good["ingress"] == {"function_url": "absent", "policy": "absent"}, "the ingress keys are absent")
    expect(good["reported_not_promised"] is True and good["schema"] == SCHEMA, "the schema and the promise flag")

    # Rounding and the 12-digit refusal.
    rounded = outcome_of([sample("cold-1", EV1, init=280.0, probe=good_probe(1, encode_ms=1000.123456))
                          for _ in range(2)] + good_samples()[3:])
    expect(rounded["samples"][0]["probe"]["encode_ms"] == 1000.123, "floats are rounded to three decimals")
    leaky = raw_run(good_samples())
    leaky["samples"][0]["probe"]["cpu_part"] = "account " + "9" * 12  # built at runtime, never a literal
    refused = False
    try:
        normalize(leaky)
    except ValueError:
        refused = True
    expect(refused, "a raw record that contains a 12-digit string is refused")
    expect(evidence_text(good).endswith("\n"), "the evidence text is newline-terminated")
    print(f"GRAVITON GATE SELFTEST OK {cases} cases")
    return 0


def main(argv):
    if len(argv) >= 2 and argv[1] == "selftest":
        return selftest()
    if len(argv) >= 3 and argv[1] == "normalize":
        with open(argv[2], encoding="utf-8") as handle:
            raw = json.load(handle)
        text = evidence_text(normalize(raw))
        if "--stdout" in argv[3:]:
            sys.stdout.write(text)
            return 0
        if "--out" in argv[3:]:
            out = argv[argv.index("--out") + 1]
            with open(out, "w", encoding="utf-8") as handle:
                handle.write(text)
            print(f"EVIDENCE WRITTEN {out}")
            return 0
    print("usage: graviton_gate.py normalize <raw.json> (--out <file> | --stdout) | selftest",
          file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
