"""Print the GGUF sha256 the capability ladder pins for one rung (D-01 / D-13).

This is the ONLY place the deployment derives the model hash from. The value is
parsed out of contracts/model-capability-ladder-v1.yaml at run time, never typed
a second time, so a re-pin is one edit to the ladder.

    python3 -I deploy/llm-endpoint/ladder_pin.py <ladder path> <rung id>

Exit codes (stdout carries the pin on 0 only; every refusal is one line on stderr):
    0  exactly one distinct sha256 is attributed to the rung; it is printed
    2  usage error, or the ladder cannot be read / parsed
    3  NO PIN: no entry for the rung carries a sha256
    4  AMBIGUOUS PIN: two or more distinct sha256 values are attributed to the rung
    5  REFUSED yaml: PyYAML is not importable
    6  BAD PIN: an entry for the rung carries a sha256 that is not 64 lowercase hex
"""

import re
import sys

HEX64 = re.compile(r"^[0-9a-f]{64}$")


def load_yaml_module():
    """Import PyYAML or refuse in one line (a documented operator dependency)."""
    try:
        import yaml  # noqa: PLC0415 - deliberate late import, see the refusal below
    except ImportError:
        print(
            "REFUSED yaml: PyYAML is not importable under python3 -I "
            "(install PyYAML 6.x for the operator Python)",
            file=sys.stderr,
        )
        sys.exit(5)
    return yaml


def string_loader(yaml):
    """A SafeLoader that keeps int/float-looking scalars as strings.

    PyYAML's implicit resolvers turn an all-digit 64-character scalar (or one
    shaped like 1234e56...) into a number, which would silently change a pin.
    """
    loader = type("PinLoader", (yaml.SafeLoader,), {})
    drop = ("tag:yaml.org,2002:int", "tag:yaml.org,2002:float")
    loader.yaml_implicit_resolvers = {
        first: [(tag, rx) for (tag, rx) in rules if tag not in drop]
        for first, rules in yaml.SafeLoader.yaml_implicit_resolvers.items()
    }
    return loader


def candidates(node, rung, found):
    """Collect every sha256 on a mapping whose `rung` or `id` equals the rung."""
    if isinstance(node, dict):
        if (node.get("rung") == rung or node.get("id") == rung) and "sha256" in node:
            found.append(node["sha256"])
        for child in node.values():
            candidates(child, rung, found)
    elif isinstance(node, list):
        for child in node:
            candidates(child, rung, found)


def main(argv):
    if len(argv) != 3:
        print("usage: ladder_pin.py <ladder path> <rung id>", file=sys.stderr)
        return 2
    path, rung = argv[1], argv[2]
    yaml = load_yaml_module()
    try:
        with open(path, encoding="utf-8") as handle:
            document = yaml.load(handle, Loader=string_loader(yaml))  # noqa: S506 - SafeLoader subclass
    except (OSError, yaml.YAMLError) as err:
        print(f"UNREADABLE ladder {path}: {type(err).__name__}", file=sys.stderr)
        return 2
    found = []
    candidates(document, rung, found)
    for value in found:
        if not isinstance(value, str) or not HEX64.match(value):
            print(f"BAD PIN for {rung}: a sha256 entry is not 64 lowercase hex", file=sys.stderr)
            return 6
    distinct = sorted(set(found))
    if not distinct:
        print(f"NO PIN for {rung}", file=sys.stderr)
        return 3
    if len(distinct) > 1:
        print(f"AMBIGUOUS PIN for {rung}: {' '.join(distinct)}", file=sys.stderr)
        return 4
    print(distinct[0])
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
