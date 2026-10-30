#!/usr/bin/env python3
"""The finalizer and state-machine failure surface: the checker of its enumeration (Phase 14, plan 14-16).

usage: laya_failure_surface.py --check [--artifact PATH] [--markdown PATH]
       laya_failure_surface.py --selftest
       laya_failure_surface.py --sweep
       laya_failure_surface.py --mutations [--only ID,ID]

The enumeration is two checked-in files: crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json (the
machine-readable surface) and 14-FAILURE-SURFACE.md (the inspection record). They were derived from the code by
inspection; this checker re-derives what CAN be re-derived and refuses any disagreement, so the enumeration cannot rot
silently:

- the axes, the catch-fed classes and the ratchet are parsed out of finalizer.rs (SURFACE_AXES, CATCH_FED,
  KNOWN_DEFECTS) and must equal the artifact's `axes`, `catch_fed` and `known_defects`;
- the exit-class map is what run_step.py's `job_exit_code` returns NOW for every class in its EXIT_CLASSES (the module
  is loaded with importlib; its entry point is guarded by a __main__ check), and `catch_fed` is exactly the classes
  that exit 1;
- every sink count is a fresh scan of the non-test text (before the first `#[cfg(test)]`) of the scanned Rust files,
  and every (file, pattern) the scan hits must be classified caller, log-only or static, with the class counts summing
  to the hit count. A new sink changes a count, or appears unclassified, and is refused;
- the state machine's states, Catch edges, timeout, status-rule statuses and reconcile rate are re-read from
  laya-trainer-stack.ts (14-19 binds them to the synthesized template as well);
- the row ids of the JSON and the markdown tables are the same set, every row names a test function that exists in
  the file it names (or is a defect whose closing plan adds it), and every invariant a row cites exists.

`--check` exits 0 with `FAILURE SURFACE OK ...`, or 1 with one `REFUSED surface-<rule>: <detail>` line per failure.
`--selftest` mutates in-memory copies once per case and requires exactly that case's own rule to refuse it, and the
unmutated inputs to pass; it ends `FAILURE SURFACE SELFTEST OK <n> cases`. `--sweep` runs the finalizer sweep tests with cargo
and refuses unless every expected test is reported ok and at least the expected minimum passed; it ends
`FAILURE SURFACE SWEEP OK`. `--mutations` is the source-mutation runner plans 14-17 and 14-18 register cases in: each
case rewrites one anchor of the fixed source on disk, requires the named tests or invariants to turn red, and restores
the file byte for byte (in a finally block, re-read to prove it); it ends `FAILURE SURFACE MUTATIONS OK <n>/<n>`.

Standard library only; run with `python3 -I`. It makes no network call.
"""

import copy
import importlib.util
import json
import os
import re
import signal
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))

ARTIFACT = "crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json"
MARKDOWN = ".planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-FAILURE-SURFACE.md"
FINALIZER = "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs"
RUN_STEP = "scripts/laya_train/container/run_step.py"
STACK = "deploy-extensions/lib/laya-trainer-stack.ts"

SCHEMA = "laya-failure-surface-v1"
REQUIRED_KEYS = (
    "schema", "derivation", "state_machine", "exit_classes", "run_objects", "decision_order", "publication",
    "sink_scope", "sink_patterns", "sinks", "invariants", "axes", "catch_fed", "known_defects", "rows",
    "tests_not_modelling_catch",
)
INVARIANT_IDS = ("I1", "I2", "I3", "I4", "I5", "I6", "I7", "I8")
ROW_ID = re.compile(r"^(?:FS-[A-Z]{2}-[0-9A-Z]+|SK-[0-9A-Z]+)$")
STATUSES = ("ok", "defect", "closed")
CLOSING_PLANS = ("14-17", "14-18", "14-19")
UNTESTED = "untested at HEAD"

# The sink scan: what is scanned, and for what. Both are recorded in the artifact and must equal these.
SCAN_DIRS = ("crates/aprender-mcp-laya-train/src", "crates/aprender-mcp-laya-train-lambda/src")
SCAN_FILES = ("crates/aprender-setfit-train-lambda/src/dynamodb.rs", "crates/aprender-mcp-setfit-train/src/task_store.rs")
SINK_PATTERNS = {
    "aws-renderer": r"DisplayErrorContext\(",
    "pmcp-validation": r"Error::validation\(",
    "pmcp-internal": r"Error::internal\(",
    "failed-payload": r"\bfailed_payload\(",
    "payload-error-assign": r"\.error\s*=\s*",
    "dispatch-error": r"\bDispatchError\s*\{|DispatchError::[A-Z]",
    "start-error": r"\bStartError\s*\{",
    "stage-error": r"\bstage_error\b",
    "log": r"\btracing::(?:error|warn|info|debug)!|\beprintln!|\bprintln!",
    "payload-notes": r"\.notes\.push\(",
}
SINK_CLASSES = ("caller", "log-only", "static")

# --sweep: the finalizer sweep tests that must run and pass. Later plans extend the table (14-18 renames the sweep).
SWEEP = {
    "crate": "aprender-mcp-laya-train-lambda",
    "filter": "failure_surface",
    "tests": [
        "failure_surface_tracer_wr01",
        "failure_surface_sweep_known_defects_only",
        "failure_surface_transient_faults",
    ],
    "min_passed": 3,
}

# --mutations: the red-side proofs. One case is (id, file, old text, new text, runs): `old` must occur in `file`
# EXACTLY once (taken verbatim from the fixed source), `new` replaces it, and every run (crate, test filter, expected)
# must then exit non-zero with output naming every expected token. A token is either a test function name (its
# `test ...::<name> ... FAILED` line) or an invariant id (the sweep's `violations outside the ratchet: [...(<id>, ...`).
# Before any mutation each distinct (crate, filter) runs unmutated and must be green with every expected test reported
# ok (the control: a filter that matches nothing exits 0 with zero tests run). Plans 14-17 and 14-18 register cases.
SERVER = "aprender-mcp-laya-train"
LAMBDA = "aprender-mcp-laya-train-lambda"
FINALIZE_RS = "crates/aprender-mcp-laya-train/src/finalize.rs"
SWEEP_TEST = "failure_surface_sweep_known_defects_only"
TRACER_TEST = "failure_surface_tracer_wr01"

MUTATIONS = [
    # WR-01: the class comes from the records, the stage error is detail (plan 14-17 Task 1).
    ("M-WR01-a", FINALIZE_RS,
     "    let Some(outcome) = input.outcome.as_ref() else {\n",
     "    if let Some(error) = &input.stage_error {\n"
     "        return b.failed(error, None, false);\n"
     "    }\n"
     "    let Some(outcome) = input.outcome.as_ref() else {\n",
     [(LAMBDA, "failure_surface", ["I3", "I8", TRACER_TEST, SWEEP_TEST])]),
    ("M-WR01-b", FINALIZE_RS,
     '            &b.with_stage("pack-outcome.json is missing: the pack stage left no record"),\n',
     '            "pack-outcome.json is missing: the pack stage left no record",\n',
     [(SERVER, "finalize_pack_missing", ["finalize_pack_missing"])]),
    ("M-WR01-c", FINALIZE_RS,
     '                    &format!("the trainer refused a dataset the pre-filter accepted: {line}"),\n'
     "                    None,\n"
     "                    true,\n",
     '                    &format!("the trainer refused a dataset the pre-filter accepted: {line}"),\n'
     "                    None,\n"
     "                    false,\n",
     [(SERVER, "finalize_refused", ["finalize_refused_with_stage_error"])]),
    ("M-WR01-d", FINALIZE_RS,
     "            payload.notes.push(NOTE_STAGE_ERROR_OVERRIDDEN.to_string());\n",
     "            let _ = NOTE_STAGE_ERROR_OVERRIDDEN;\n",
     [(SERVER, "finalize_stage_error_overridden", ["finalize_stage_error_overridden"]),
      (LAMBDA, SWEEP_TEST, ["I3"])]),
    # WR-02 and IN-01: finalization is total over the run's objects, every inventory is tagged (Task 2).
    ("M-WR02-a", FINALIZER,
     "        drop_absent_entries(ctx, &subject.run_prefix, &mut objects).await?;\n",
     "        let _ = &mut objects;\n",
     [(LAMBDA, SWEEP_TEST, ["I2"])]),
    ("M-WR02-b", FINALIZER,
     "    targets.retain(|key| present.contains(key));\n",
     "    let _ = &present;\n",
     [(LAMBDA, SWEEP_TEST, ["I2"])]),
    ("M-WR02-c", FINALIZER,
     '    targets.extend(SELF_LISTING.iter().map(|name| format!("{prefix}{name}")));\n',
     "",
     [(LAMBDA, SWEEP_TEST, ["I7"])]),
    # The wrong fix for an adopted manifest that names a gone object: rebuild its entries from the listing, which
    # tags keys no inventory names.
    ("M-WR02-d", FINALIZER,
     "    let manifest = if let Some(existing) = adopted {\n"
     "        existing\n",
     "    let manifest = if let Some(mut existing) = adopted {\n"
     "        existing.entries = ctx\n"
     "            .objects\n"
     "            .list(prefix)\n"
     "            .await\n"
     '            .map_err(|e| store_err(format!("listing {prefix}: {e}")))?\n'
     "            .into_iter()\n"
     "            .filter_map(|(key, size)| {\n"
     "                Some(ManifestEntry {\n"
     "                    path: key.strip_prefix(prefix.as_str())?.to_string(),\n"
     "                    size,\n"
     "                    sha256: String::new(),\n"
     "                })\n"
     "            })\n"
     "            .collect();\n"
     "        existing\n",
     [(LAMBDA, "fin_adopted_manifest_with_absent_entry", ["fin_adopted_manifest_with_absent_entry"])]),
    ("M-WR02-e", FINALIZE_RS,
     "        if let Some(note) = dropped_note(&self.input.dropped) {\n"
     "            payload.notes.push(note);\n"
     "        }\n",
     "        let _ = dropped_note(&self.input.dropped);\n",
     [(SERVER, "finalize_dropped_note", ["finalize_dropped_note"]),
      (LAMBDA, SWEEP_TEST, ["I2"])]),
]

def read(rel, root=ROOT):
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        return f.read()


class Report:
    def __init__(self):
        self.refused = []
        self.count = 0

    def check(self, rule, ok, detail=""):
        self.count += 1
        if not ok:
            self.refused.append((rule, detail))
        return ok


# --------------------------------------------------------------------------------------------- re-derivations

def const_block(text, name):
    """The bracketed initializer of `const NAME: ... = &[ ... ];` in Rust source, comments stripped, or None."""
    m = re.search(r"\bconst %s\s*:[^=]*=\s*&?\[" % re.escape(name), text)
    if m is None:
        return None
    start, depth = m.end() - 1, 0
    for j in range(start, len(text)):
        if text[j] == "[":
            depth += 1
        elif text[j] == "]":
            depth -= 1
            if depth == 0:
                return re.sub(r"//[^\n]*", "", text[start:j + 1])
    return None


def rust_axes(text):
    block = const_block(text, "SURFACE_AXES")
    if block is None:
        return None
    rows = re.findall(r"\(\s*\"([a-z_]+)\"\s*,\s*&\[(.*?)\]\s*,?\s*\)", block, re.S)
    return [[name, re.findall(r"\"([a-z0-9_]+)\"", values)] for name, values in rows]


def rust_catch_fed(text):
    block = const_block(text, "CATCH_FED")
    return None if block is None else re.findall(r"\"([a-z_]+)\"", block)


def rust_known_defects(text):
    block = const_block(text, "KNOWN_DEFECTS")
    return None if block is None else re.findall(r"Invariant::(I[0-9]+)", block)


def exit_map(root=ROOT):
    """{class: job_exit_code(class)} for every class of run_step.py's EXIT_CLASSES, by RUNNING the function."""
    spec = importlib.util.spec_from_file_location("laya_run_step_surface", os.path.join(root, RUN_STEP))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return {c: mod.job_exit_code(c) for c in mod.EXIT_CLASSES}


def scan_files(root=ROOT):
    rels = []
    for d in SCAN_DIRS:
        for dirpath, dirnames, filenames in os.walk(os.path.join(root, d)):
            dirnames.sort()
            for fn in sorted(filenames):
                if fn.endswith(".rs"):
                    rels.append(os.path.relpath(os.path.join(dirpath, fn), root))
    rels.extend(SCAN_FILES)
    return sorted(set(rels))


def load_sources(root=ROOT):
    return {rel: read(rel, root) for rel in scan_files(root)}


def non_test(text):
    i = text.find("#[cfg(test)]")
    return text if i < 0 else text[:i]


def scan(sources):
    """{(file, pattern): hits} over the non-test text of every scanned file; zero counts are omitted."""
    out = {}
    for rel in sorted(sources):
        body = non_test(sources[rel])
        for name, rx in SINK_PATTERNS.items():
            n = len(re.findall(rx, body))
            if n:
                out[(rel, name)] = n
    return out


def ts_machine(ts):
    """The state-machine facts re-read from the CDK source, or a string naming what could not be read."""
    var_state = dict(re.findall(r"const (\w+) = new tasks\.\w+\(this, '\w+', \{\s*stateName: '(\w+)'", ts))
    states = re.findall(r"stateName: '(\w+)'", ts)
    catch = {}
    for var, target, rp in re.findall(
            r"(\w+)\.addCatch\((\w+), \{ errors: \[sfn\.Errors\.ALL\], resultPath: '([^']+)' \}\)", ts):
        if var not in var_state or target not in var_state:
            return "an addCatch names a state variable the source does not declare (%s -> %s)" % (var, target)
        catch.setdefault(var_state[var], []).append(
            {"ErrorEquals": ["States.ALL"], "Next": var_state[target], "ResultPath": rp})
    timeout = re.search(r"export const MACHINE_TIMEOUT_SECS = ([0-9_]+);", ts)
    uses_timeout = re.search(r"timeout: cdk\.Duration\.seconds\(MACHINE_TIMEOUT_SECS\)", ts)
    statuses = re.search(r"status: \[([^\]]*)\]", ts)
    rate = re.search(r"Schedule\.rate\(cdk\.Duration\.minutes\((\d+)\)\)", ts)
    if not (states and timeout and uses_timeout and statuses and rate):
        return "the source no longer declares the states, the machine timeout, the status rule or the schedule"
    return {
        "states": states,
        "catch": catch,
        "no_catch_states": [s for s in states if s not in catch],
        "machine_timeout_secs": int(timeout.group(1).replace("_", "")),
        "event_statuses": re.findall(r"'([A-Z_]+)'", statuses.group(1)),
        "reconcile_rate_minutes": int(rate.group(1)),
    }


def markdown_row_ids(md):
    """The ids in the first cell of every markdown table row (prose mentions do not count)."""
    ids = []
    for line in md.splitlines():
        m = re.match(r"\|\s*([A-Z]{2}-[A-Z0-9-]+)\s*\|", line)
        if m and ROW_ID.match(m.group(1)):
            ids.append(m.group(1))
    return ids


def test_exists(ref, root=ROOT, cache=None):
    path = os.path.join(root, ref.get("file", ""))
    if not ref.get("file") or not ref.get("name") or not os.path.isfile(path):
        return False
    text = cache.get(path) if cache is not None and path in cache else read(ref["file"], root)
    if cache is not None:
        cache[path] = text
    name = re.escape(ref["name"])
    if path.endswith(".rs"):
        return re.search(r"\bfn %s\s*\(" % name, text) is not None
    if path.endswith(".py"):
        return re.search(r"^\s*def %s\s*\(" % name, text, re.M) is not None
    return False


# ------------------------------------------------------------------------------------------------ the check

class Inputs:
    """Everything one check reads. The selftest mutates copies of these, never a file."""

    def __init__(self, artifact, markdown, finalizer, sources, exits, ts, root=ROOT):
        self.artifact = artifact
        self.markdown = markdown
        self.finalizer = finalizer
        self.sources = sources
        self.exits = exits
        self.ts = ts
        self.root = root


def load_inputs(artifact_path=None, markdown_path=None, root=ROOT):
    artifact_path = artifact_path or os.path.join(root, ARTIFACT)
    markdown_path = markdown_path or os.path.join(root, MARKDOWN)
    with open(artifact_path, encoding="utf-8") as f:
        artifact = json.load(f)
    with open(markdown_path, encoding="utf-8") as f:
        markdown = f.read()
    return Inputs(artifact, markdown, read(FINALIZER, root), load_sources(root), exit_map(root), read(STACK, root),
                  root)


def check(inp):
    rep = Report()
    a = inp.artifact

    # schema
    missing = [k for k in REQUIRED_KEYS if k not in a]
    rep.check("surface-schema", a.get("schema") == SCHEMA and not missing,
              "schema %r, missing keys %s" % (a.get("schema"), missing))
    if missing:
        return rep

    # rows: ids, markdown, statuses, tests, invariants
    rows = a["rows"] if isinstance(a["rows"], list) else []
    ids = [r.get("id") for r in rows]
    dup = sorted({i for i in ids if ids.count(i) > 1})
    bad = [i for i in ids if not isinstance(i, str) or not ROW_ID.match(i)]
    rep.check("surface-row-ids", rows and not dup and not bad, "duplicated %s, malformed %s" % (dup, bad))
    md_ids = markdown_row_ids(inp.markdown)
    md_dup = sorted({i for i in md_ids if md_ids.count(i) > 1})
    only_json, only_md = sorted(set(ids) - set(md_ids) - {None}), sorted(set(md_ids) - set(ids))
    rep.check("surface-rows-markdown", not only_json and not only_md and not md_dup,
              "only in the JSON %s, only in the markdown %s, repeated in the markdown %s" % (only_json, only_md, md_dup))

    inv_ids = [i.get("id") for i in a["invariants"]] if isinstance(a["invariants"], list) else []
    rep.check("surface-invariants", inv_ids == list(INVARIANT_IDS) and all(i.get("text") for i in a["invariants"]),
              "invariants %s are not I1..I8, each with its text" % inv_ids)
    cache = {}
    for r in rows:
        rid = r.get("id")
        status = r.get("status")
        ok_status = status in STATUSES and (
            status != "defect" or (re.fullmatch(r"(WR|IN)-[0-9]{2}", str(r.get("ref"))) and r.get("closes") in CLOSING_PLANS))
        rep.check("surface-row-status", ok_status,
                  "%s: status %r ref %r closes %r (a defect names a WR/IN finding and one of %s)"
                  % (rid, status, r.get("ref"), r.get("closes"), list(CLOSING_PLANS)))
        cited = r.get("invariants", [])
        unknown = [c for c in cited if c not in inv_ids]
        rep.check("surface-invariant-cited", isinstance(cited, list) and not unknown,
                  "%s cites unknown invariants %s" % (rid, unknown))
        test = r.get("test")
        if test is None:
            # No test exercises this row at HEAD. That is recorded, never papered over with an unrelated name: the
            # row must say so in its note, and the OK line counts it.
            rep.check("surface-test-exists", str(r.get("note", "")).startswith(UNTESTED),
                      "%s names no covering test and its note does not start %r" % (rid, UNTESTED))
        else:
            rep.check("surface-test-exists", test_exists(test, inp.root, cache),
                      "%s names %s::%s, which does not exist" % (rid, test.get("file"), test.get("name")))
    for t in a["tests_not_modelling_catch"]:
        rep.check("surface-test-exists", test_exists(t, inp.root, cache),
                  "tests_not_modelling_catch names %s::%s, which does not exist" % (t.get("file"), t.get("name")))

    # axes, catch_fed, known_defects: bound to the Rust consts
    axes = rust_axes(inp.finalizer)
    rep.check("surface-axes", axes is not None and a["axes"] == axes,
              "the artifact's axes %s differ from SURFACE_AXES in finalizer.rs %s" % (a["axes"], axes))
    fed = rust_catch_fed(inp.finalizer)
    exit_one = sorted(c for c, code in inp.exits.items() if code == 1)
    rep.check("surface-catch-fed", fed is not None and a["catch_fed"] == fed and sorted(fed) == exit_one,
              "catch_fed %s, CATCH_FED in finalizer.rs %s, classes job_exit_code exits 1 for %s"
              % (a["catch_fed"], fed, exit_one))
    kd = rust_known_defects(inp.finalizer)
    art_kd = [k.get("id") for k in a["known_defects"]] if isinstance(a["known_defects"], list) else None
    kd_ok = kd is not None and art_kd == kd and all(
        k.get("id") in inv_ids and re.fullmatch(r"(WR|IN)-[0-9]{2}", str(k.get("ref"))) and k.get("closes") in CLOSING_PLANS
        for k in a["known_defects"])
    rep.check("surface-known-defects", kd_ok,
              "known_defects %s differ from KNOWN_DEFECTS in finalizer.rs %s (or lack a WR/IN ref and closing plan)"
              % (art_kd, kd))

    # the exit-class map, re-derived by running job_exit_code
    rep.check("surface-exit-class", a["exit_classes"].get("job_exit_code") == inp.exits,
              "the artifact's job_exit_code map %s differs from run_step.py now %s"
              % (a["exit_classes"].get("job_exit_code"), inp.exits))

    # the sinks, against a fresh scan
    rep.check("surface-sink-scope", a["sink_scope"] == {"dirs": list(SCAN_DIRS), "files": list(SCAN_FILES)},
              "sink_scope %s is not the checker's scan scope" % a["sink_scope"])
    rep.check("surface-sink-patterns", a["sink_patterns"] == SINK_PATTERNS,
              "sink_patterns differ from the checker's SINK_PATTERNS")
    fresh = scan(inp.sources)
    row_set = set(ids)
    seen = set()
    for s in a["sinks"]:
        key = (s.get("file"), s.get("pattern"))
        seen.add(key)
        classes = s.get("classes", {})
        got = fresh.get(key, 0)
        rep.check("surface-sink-count", s.get("count") == got,
                  "%s %s: the artifact records %s, a fresh scan finds %d" % (key[0], key[1], s.get("count"), got))
        rep.check("surface-sink-class",
                  isinstance(classes, dict) and set(classes) <= set(SINK_CLASSES) and sum(classes.values()) == s.get("count"),
                  "%s %s: classes %s do not sum to %s or use a class outside %s"
                  % (key[0], key[1], classes, s.get("count"), list(SINK_CLASSES)))
        rep.check("surface-sink-rows", s.get("rows") and all(x in row_set for x in s.get("rows", [])),
                  "%s %s: rows %s are not all row ids" % (key[0], key[1], s.get("rows")))
    for key in sorted(set(fresh) - seen):
        rep.check("surface-sink-unclassified", False,
                  "%s %s: %d hit(s) the artifact does not classify" % (key[0], key[1], fresh[key]))

    # the state machine, re-read from the CDK source
    facts = ts_machine(inp.ts)
    sm = a["state_machine"]
    if isinstance(facts, str):
        rep.check("surface-state-machine", False, facts)
    else:
        diff = [k for k in facts if sm.get(k) != facts[k]]
        rep.check("surface-state-machine", not diff,
                  "fields %s differ from %s: %s" % (diff, STACK, {k: (sm.get(k), facts[k]) for k in diff}))
    return rep


def report_line(inp, rep):
    a = inp.artifact
    fresh = scan(inp.sources)
    cells = 1
    for _, values in a["axes"]:
        cells *= len(values)
    untested = sum(1 for r in a["rows"] if r.get("test") is None)
    return ("FAILURE SURFACE OK rows=%d (untested at HEAD %d) invariants=%d sinks=%d (hits %d) axes=%d cells=%d "
            "known_defects=%s assertions=%d"
            % (len(a["rows"]), untested, len(a["invariants"]), len(a["sinks"]), sum(fresh.values()), len(a["axes"]),
               cells, ",".join(k["id"] for k in a["known_defects"]) or "none", rep.count))


# ------------------------------------------------------------------------------------------------ selftest

def mutate_artifact(fn):
    def apply(inp):
        fn(inp.artifact)
    return apply


def drop_markdown_row(inp):
    rid = inp.artifact["rows"][0]["id"]
    lines = inp.markdown.splitlines()
    kept = [l for l in lines if not re.match(r"\|\s*%s\s*\|" % re.escape(rid), l)]
    if len(kept) == len(lines):
        raise ValueError("row %s has no markdown table line" % rid)
    inp.markdown = "\n".join(kept)


def bump_sink(a):
    # A stale but self-consistent record (its classes still sum): only the fresh scan can disagree.
    s = a["sinks"][0]
    s["count"] += 1
    k = next(iter(s["classes"]))
    s["classes"][k] += 1


def flip_exit(a):
    m = a["exit_classes"]["job_exit_code"]
    m["refused"] = 0 if m["refused"] else 1


def cite_unknown(a):
    a["rows"][0].setdefault("invariants", []).append("I9")


def missing_test(a):
    row = next(r for r in a["rows"] if r.get("test"))
    row["test"] = dict(row["test"], name="no_such_test_fn_14_16")


def edit_axis(a):
    a["axes"][1][1][0] = "vanished"


def drop_defect(a):
    a["known_defects"].pop()


def dup_row(a):
    a["rows"].append(copy.deepcopy(a["rows"][0]))


def drop_key(a):
    del a["publication"]


def new_sink(inp):
    rel = "crates/aprender-mcp-laya-train/src/lock.rs"
    if rel not in inp.sources:
        raise ValueError("%s is not scanned" % rel)
    inp.sources[rel] = "fn leak(e: &E) -> String { DisplayErrorContext(e).to_string() }\n" + inp.sources[rel]


def fed_drift(inp):
    inp.finalizer = inp.finalizer.replace('const CATCH_FED: &[&str] = &["refused", "crash", "timeout"];',
                                          'const CATCH_FED: &[&str] = &["refused", "crash"];', 1)
    if 'const CATCH_FED: &[&str] = &["refused", "crash"];' not in inp.finalizer:
        raise ValueError("the CATCH_FED line was not found")


def wrong_class_sum(a):
    s = a["sinks"][0]
    k = next(iter(s["classes"]))
    s["classes"][k] += 1


def add_state(a):
    a["state_machine"]["states"].append("Retrain")


def defect_without_plan(a):
    row = next(r for r in a["rows"] if r.get("status") == "defect")
    row["closes"] = None


CASES = [
    ("a row id dropped from the markdown", drop_markdown_row, "surface-rows-markdown"),
    ("a sink count changed", mutate_artifact(bump_sink), "surface-sink-count"),
    ("an exit-class entry flipped", mutate_artifact(flip_exit), "surface-exit-class"),
    ("a row cites an unknown invariant", mutate_artifact(cite_unknown), "surface-invariant-cited"),
    ("a row names a missing test", mutate_artifact(missing_test), "surface-test-exists"),
    ("an axis value edited", mutate_artifact(edit_axis), "surface-axes"),
    ("a known defect dropped", mutate_artifact(drop_defect), "surface-known-defects"),
    ("a row id duplicated", mutate_artifact(dup_row), "surface-row-ids"),
    ("a schema key removed", mutate_artifact(drop_key), "surface-schema"),
    ("a new sink in an unclassified file", new_sink, "surface-sink-unclassified"),
    ("CATCH_FED edited in finalizer.rs", fed_drift, "surface-catch-fed"),
    ("a sink's class counts do not sum", mutate_artifact(wrong_class_sum), "surface-sink-class"),
    ("a state added to the artifact's machine", mutate_artifact(add_state), "surface-state-machine"),
    ("a defect row without a closing plan", mutate_artifact(defect_without_plan), "surface-row-status"),
]


def selftest():
    base = load_inputs()
    fails = 0
    rep = check(base)
    ok = not rep.refused
    print("%s the unmutated artifact passes every assertion (%d)" % ("  ok" if ok else "  FAIL", rep.count))
    for rule, detail in rep.refused:
        print("       REFUSED %s: %s" % (rule, detail))
    fails += 0 if ok else 1
    for name, mutate, want in CASES:
        inp = Inputs(copy.deepcopy(base.artifact), base.markdown, base.finalizer, dict(base.sources),
                     dict(base.exits), base.ts, base.root)
        try:
            mutate(inp)
        except Exception as e:  # a mutation that cannot be applied proves nothing
            print("  FAIL %s: the mutation could not be applied: %r" % (name, e))
            fails += 1
            continue
        got = {r for r, _ in check(inp).refused}
        if got == {want}:
            print("  ok %s -> REFUSED %s" % (name, want))
        else:
            # Refused for another rule as well (or instead) means the case does not prove ITS rule can fail.
            print("  FAIL %s: expected exactly REFUSED %s, got %s" % (name, want, sorted(got) or "no refusal"))
            fails += 1
    print("%d mutation cases, %d failure(s)" % (len(CASES), fails))
    if fails:
        print("FAILURE SURFACE SELFTEST FAILED")
        return 1
    print("FAILURE SURFACE SELFTEST OK %d cases" % len(CASES))
    return 0


# --------------------------------------------------------------------------------------------- sweep, mutations

def sweep():
    cmd = ["cargo", "test", "-p", SWEEP["crate"], "--lib", SWEEP["filter"]]
    print("+ " + " ".join(cmd))
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    out = r.stdout
    refused = []
    if r.returncode != 0:
        refused.append("cargo exited %d" % r.returncode)
    for name in SWEEP["tests"]:
        if not re.search(r"^test \S*::%s \.\.\. ok$" % re.escape(name), out, re.M):
            refused.append("%s was not reported ok" % name)
    passed = [int(n) for n in re.findall(r"^test result: \w+\. (\d+) passed", out, re.M)]
    total = sum(passed)
    if total < SWEEP["min_passed"]:
        refused.append("%d passed, below the minimum %d" % (total, SWEEP["min_passed"]))
    for line in re.findall(r"^test \S*failure_surface\S* \.\.\. \S+$", out, re.M):
        print("  " + line)
    if refused:
        for x in refused:
            print("REFUSED surface-sweep: %s" % x)
        tail = (r.stdout + r.stderr).strip().splitlines()[-12:]
        for line in tail:
            print("  | " + line)
        print("FAILURE SURFACE SWEEP FAILED")
        return 1
    print("FAILURE SURFACE SWEEP OK %d/%d named tests ok, %d passed (minimum %d)"
          % (len(SWEEP["tests"]), len(SWEEP["tests"]), total, SWEEP["min_passed"]))
    return 0


INVARIANT_TOKEN = re.compile(r"^I[0-9]+$")


def cargo_test(crate, filt):
    cmd = ["cargo", "test", "-p", crate, "--lib", filt]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    # stderr (the build) first, so a diagnostic tail shows the test lines.
    return r.returncode, r.stderr + "\n" + r.stdout


def names_token(out, token, outcome):
    """True when `out` reports `token` with `outcome` (FAILED or ok); an invariant id is named by the sweep's ratchet
    assertion (only a failing sweep prints it)."""
    if INVARIANT_TOKEN.match(token):
        return outcome == "FAILED" and re.search(r"violations outside the ratchet: .*\(%s," % token, out) is not None
    return re.search(r"^test \S*::%s \.\.\. %s$" % (re.escape(token), outcome), out, re.M) is not None


class Interrupted(Exception):
    """A SIGTERM or SIGHUP, raised so the restoring finally block runs."""


def _interrupt(signum, _frame):
    raise Interrupted("signal %d" % signum)


def restore(path, original):
    with open(path, "wb") as f:
        f.write(original)
    with open(path, "rb") as f:
        back = f.read()
    if back != original:
        raise RuntimeError("%s was NOT restored byte for byte" % path)


def mutations(only):
    known = [m[0] for m in MUTATIONS]
    unknown = sorted(set(only) - set(known))
    if unknown:
        print("REFUSED usage: --only names mutations that are not registered: %s" % ", ".join(unknown), file=sys.stderr)
        return 2
    if not MUTATIONS:
        print("FAILURE SURFACE MUTATIONS: none registered yet (plans 14-17 and 14-18 register them); nothing was run")
        return 0
    if len(set(known)) != len(known):
        print("REFUSED mutations: a mutation id is registered twice", file=sys.stderr)
        return 1
    cases = [m for m in MUTATIONS if not only or m[0] in only]
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, _interrupt)

    # Every file the selected cases touch, as it is now: the run must end with each one byte-identical to this.
    before = {}
    for _, rel, _, _, _ in cases:
        with open(os.path.join(ROOT, rel), "rb") as f:
            before.setdefault(rel, f.read())

    # The controls: each distinct (crate, filter), unmutated, green, with every expected test reported ok.
    controls = {}
    for _, _, _, _, runs in cases:
        for crate, filt, tokens in runs:
            controls.setdefault((crate, filt), set()).update(t for t in tokens if not INVARIANT_TOKEN.match(t))
    failures = []
    for (crate, filt), tests in sorted(controls.items()):
        rc, out = cargo_test(crate, filt)
        missing = sorted(t for t in tests if not names_token(out, t, "ok"))
        passed = sum(int(n) for n in re.findall(r"^test result: \w+\. (\d+) passed", out, re.M))
        ok = rc == 0 and not missing and passed > 0
        print("CONTROL %s %s: %s" % (crate, filt, "green, %d passed" % passed if ok else
                                    "NOT green (rc %d, %d passed, not reported ok: %s)" % (rc, passed, missing)))
        if not ok:
            failures.append("control %s %s" % (crate, filt))
    if failures:
        print("FAILURE SURFACE MUTATIONS FAILED: a control run is not green, so no mutation can prove anything")
        return 1

    refused = 0
    for mid, rel, old, new, runs in cases:
        path = os.path.join(ROOT, rel)
        with open(path, "rb") as f:
            original = f.read()
        text = original.decode("utf-8")
        hits = text.count(old)
        if hits != 1:
            print("MUTATION %s INVALID: the old text occurs %d times in %s (it must occur exactly once)" % (mid, hits, rel))
            failures.append(mid)
            continue
        verdicts = []
        try:
            with open(path, "wb") as f:
                f.write(text.replace(old, new, 1).encode("utf-8"))
            for crate, filt, tokens in runs:
                rc, out = cargo_test(crate, filt)
                unnamed = [t for t in tokens if not names_token(out, t, "FAILED")]
                verdicts.append((crate, filt, rc, tokens, unnamed, out))
        finally:
            restore(path, original)
        bad = [v for v in verdicts if v[2] == 0 or v[4]]
        if bad:
            failures.append(mid)
            for crate, filt, rc, tokens, unnamed, out in bad:
                print("MUTATION %s SURVIVED in %s %s: rc %d, expected red %s, not named %s"
                      % (mid, crate, filt, rc, tokens, unnamed))
                for line in out.strip().splitlines()[-8:]:
                    print("  | " + line)
            continue
        refused += 1
        print("MUTATION %s %s: refused by %s; restored" % (
            mid, rel, "; ".join("%s %s -> %s" % (c, f, ", ".join(t)) for c, f, _, t, _, _ in verdicts)))

    # The tree the runner found is the tree it leaves.
    for rel, original in sorted(before.items()):
        with open(os.path.join(ROOT, rel), "rb") as f:
            if f.read() != original:
                print("FAILURE SURFACE MUTATIONS FAILED: %s differs from the pre-run bytes" % rel)
                return 1
    if failures:
        print("FAILURE SURFACE MUTATIONS FAILED %d/%d refused; failed: %s" % (refused, len(cases), ", ".join(failures)))
        return 1
    print("FAILURE SURFACE MUTATIONS OK %d/%d" % (refused, len(cases)))
    return 0


# ---------------------------------------------------------------------------------------------------- main

USAGE = ("usage: laya_failure_surface.py --check [--artifact PATH] [--markdown PATH] | --selftest | --sweep"
         " | --mutations [--only ID,ID]")


def main(argv):
    args = argv[1:]
    if not args or args[0] not in ("--check", "--selftest", "--sweep", "--mutations"):
        print(USAGE, file=sys.stderr)
        return 2
    mode, rest = args[0], args[1:]
    opts = {}
    while rest:
        flag = rest.pop(0)
        allowed = {"--check": ("--artifact", "--markdown"), "--mutations": ("--only",)}.get(mode, ())
        if flag not in allowed or not rest:
            print("REFUSED usage: %s does not take %r (or it lacks a value)\n%s" % (mode, flag, USAGE), file=sys.stderr)
            return 2
        opts[flag] = rest.pop(0)
    if mode == "--selftest":
        return selftest()
    if mode == "--sweep":
        return sweep()
    if mode == "--mutations":
        only = [x for x in opts.get("--only", "").split(",") if x]
        return mutations(only)
    for flag in ("--artifact", "--markdown"):
        if flag in opts and not os.path.isfile(opts[flag]):
            print("REFUSED usage: %s %s does not exist" % (flag, opts[flag]), file=sys.stderr)
            return 2
    try:
        inp = load_inputs(opts.get("--artifact"), opts.get("--markdown"))
    except (OSError, ValueError) as e:
        print("REFUSED surface-schema: the enumeration could not be read: %s" % e)
        return 1
    rep = check(inp)
    for rule, detail in rep.refused:
        print("REFUSED %s: %s" % (rule, detail))
    if rep.refused:
        print("FAILURE SURFACE CHECK FAILED: %d refusal(s)" % len(rep.refused))
        return 1
    print(report_line(inp, rep))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
