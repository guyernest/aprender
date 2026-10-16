#!/usr/bin/env bash
# make-variant.sh <variant> <sdk_dir>
#
# Generate a BUILDABLE driver directory that points the tracked Cargo.toml template's three SDK path
# dependencies (pmcp-agent, pmcp-package, pmcp) at <sdk_dir>.
#
# Why a directory and not a renamed manifest: cargo accepts --manifest-path only for a file named
# exactly `Cargo.toml` ("the manifest-path must be a path to a Cargo.toml file"), so a sibling
# `Cargo.branch.toml` can never build. The generated directory holds a real Cargo.toml plus `src`
# and `spike029` symlinks to the tracked sources:
#
#   bash deploy/llm-endpoint/probe/agent-loop/make-variant.sh branch ~/Development/mcp/sdk/rust-mcp-sdk-wt-content-fix
#   cargo build --release --manifest-path deploy/llm-endpoint/.ids/agent-loop-branch/Cargo.toml
#
# Output lands under the gitignored deploy/llm-endpoint/.ids/ (override: LLM_ENDPOINT_IDS_DIR).
# Prove the mechanism with `cargo metadata --no-deps --format-version 1 --manifest-path <out>/Cargo.toml`.
#
# Exit 2 with `REFUSED variant` / `REFUSED sdk` / `REFUSED usage` on bad input.
set -euo pipefail

refuse() {
  printf '%s\n' "$1" > /dev/stderr
  exit 2
}

if [ "$#" -ne 2 ]; then
  refuse "REFUSED usage: make-variant.sh VARIANT SDK_DIR"
fi
variant="$1"
sdk_dir="$2"

if ! printf '%s' "$variant" | grep -Eq '^[a-z][a-z0-9-]*$'; then
  refuse "REFUSED variant: $variant must match the pattern for a lower-case slug"
fi
sdk_manifest="$sdk_dir/crates/pmcp-agent/Cargo.toml"
if [[ ! -f "$sdk_manifest" ]]; then
  refuse "REFUSED sdk: $sdk_manifest does not exist"
fi

sdk_abs="$(cd "$sdk_dir" && pwd -P)"
here="$(cd "$(dirname "$0")" && pwd -P)"
repo_root="$(cd "$here/../../../.." && pwd -P)"
ids_dir="${LLM_ENDPOINT_IDS_DIR:-"$repo_root/deploy/llm-endpoint/.ids"}"
out="$ids_dir/agent-loop-$variant"
template="$here/Cargo.toml"

if [[ "$out" == *".."* ]]; then
  refuse "REFUSED variant: the output path must not contain '..'"
fi
mkdir -p "$out"

# Derive the template's SDK prefix (the text between `path = "` and `/crates/pmcp-agent`) and replace
# EVERY occurrence with the absolute SDK tree. The prefix contains `.` and `/`, which is why the
# replacement is a literal str.replace in python and not a sed expression.
prefix_line="$(grep -m1 -F 'crates/pmcp-agent' "$template")"
prefix="${prefix_line#*path = \"}"
prefix="${prefix%%/crates/pmcp-agent*}"
if [[ -z "$prefix" || "${#prefix}" -gt 200 ]]; then
  refuse "REFUSED template: could not derive the SDK prefix from $template"
fi
python3 -I -c 'import sys; t = open(sys.argv[1], encoding="utf-8").read(); open(sys.argv[2], "w", encoding="utf-8").write(t.replace(sys.argv[3], sys.argv[4]))' "$template" "$out/Cargo.toml" "$prefix" "$sdk_abs"

# Absolute-path symlinks to the tracked sources (replaced if they already exist).
python3 -I -c 'import os, sys; out, here = sys.argv[1], sys.argv[2]; [(os.path.lexists(os.path.join(out, n)) and os.remove(os.path.join(out, n)), os.symlink(os.path.join(here, n), os.path.join(out, n))) for n in ("src", "spike029")]' "$out" "$here"

# Verify the generated manifest: SDK_ABS appears at least three times (one per path dependency).
n="$(grep -cF "$sdk_abs" "$out/Cargo.toml" || true)"
if [[ "$n" -lt 3 ]]; then
  refuse "REFUSED manifest: expected the SDK path at least 3 times, found $n"
fi
if grep -Fq "$prefix" "$out/Cargo.toml"; then
  refuse "REFUSED manifest: the template prefix survived in $out/Cargo.toml"
fi

echo "VARIANT OK $variant $out/Cargo.toml sdk=$sdk_abs"
