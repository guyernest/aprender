"""Compare readback-*.json records written by `just llm-endpoint-readback`.

Usage: readback_equal.py A.json B.json [C.json ...]
Exit 0 when every file equals A on the compared fields; exit 1 and print
`DIFFERS <field>: <A value> != <file> <value>` for the first difference;
exit 2 on a missing/unreadable file or a file lacking a compared field.
`read_at` is deliberately not compared. Stdlib only, no network.
"""
import json
import sys

FIELDS = (
    "function_name", "state", "memory_mb", "timeout_s", "ephemeral_mb", "architecture",
    "package_type", "code_sha256", "image_uri", "last_modified", "revision_id",
    "env_keys", "reserved_concurrency", "function_url",
)


def load(path):
    try:
        with open(path) as f:
            rec = json.load(f)
    except (OSError, ValueError) as e:
        print("ERROR %s: %s" % (path, e), file=sys.stderr)
        sys.exit(2)
    if not isinstance(rec, dict):
        print("ERROR %s: not a JSON object" % path, file=sys.stderr)
        sys.exit(2)
    for field in FIELDS:
        if field not in rec:
            print("ERROR %s: lacks field %s" % (path, field), file=sys.stderr)
            sys.exit(2)
    if not isinstance(rec["env_keys"], list):
        print("ERROR %s: env_keys is not a list" % path, file=sys.stderr)
        sys.exit(2)
    return rec


def value(rec, field):
    return sorted(rec[field]) if field == "env_keys" else rec[field]


def main(argv):
    if len(argv) < 2:
        print("usage: readback_equal.py A.json B.json [C.json ...]", file=sys.stderr)
        return 2
    recs = [(p, load(p)) for p in argv]
    base = recs[0][1]
    for path, rec in recs[1:]:
        for field in FIELDS:
            if value(base, field) != value(rec, field):
                print("DIFFERS %s: %s != %s %s" % (field, value(base, field), path, value(rec, field)))
                return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
