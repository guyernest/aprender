#!/usr/bin/env bash
# Self-test for deploy/llm-endpoint/entrypoint.sh (phase 9, D-13). Offline: every external tool the
# entrypoint calls is a shim in a temp directory, so no AWS call, no model and no docker is needed
# for the case table. An eight-row table proves each refusal fires (and the positive controls pass),
# then the auth refusal is proven inside the REAL image.
#
#   bash deploy/llm-endpoint/entrypoint-selftest.sh          # table + image proof
#   SKIP_IMAGE=1 bash deploy/llm-endpoint/entrypoint-selftest.sh   # table only
#   ENTRYPOINT_UNDER_TEST=/path/to/mutated.sh SKIP_IMAGE=1 bash ...   # mutation probe: must FAIL
#
# The shims replace `date` (macOS has no %N), `nproc` and `sha256sum` (macOS has shasum), plus
# `apr` and `aws`. The shim `apr` prints `EXEC OK <argv>` only for `serve`, so a refusal row can
# assert the entrypoint never reached the exec.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ENTRY="${ENTRYPOINT_UNDER_TEST:-$HERE/entrypoint.sh}"
[ -f "$ENTRY" ] || { echo "FAIL: entrypoint not found: $ENTRY" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
SHIMS="$TMP/shims"
WORK="$TMP/work"
mkdir -p "$SHIMS" "$WORK"

# write_shim <name> <script line>...: an executable bash shim in $SHIMS. Lines are written verbatim
# (single-quoted at the call site, so nothing expands until the shim runs).
write_shim() {
  name="$1"
  shift
  printf '%s\n' '#!/usr/bin/env bash' "$@" > "$SHIMS/$name"
  chmod 0755 "$SHIMS/$name"
}
write_shim apr \
  'case "${1:-}" in serve) echo "EXEC OK $*"; exit 0 ;; esac ' \
  'echo "apr shim 0.0.0"'
# aws s3 cp <uri> <dest> [flags]: record argv, copy the fixture to <dest>
write_shim aws \
  'echo "$*" >> "$SHIM_AWS_LOG"' \
  'case "${1:-} ${2:-}" in "s3 cp") cp "$SHIM_FIXTURE" "$4"; exit 0 ;; esac ' \
  'exit 1'
write_shim sha256sum 'exec shasum -a 256 "$@"'
write_shim date \
  'case "${1:-}" in "+%s%N") python3 -I -c "import time; print(time.time_ns())" ;; *) exec /bin/date "$@" ;; esac '
write_shim nproc 'echo 4'

FIXTURE="$TMP/fixture.gguf"
printf 'phase 9 entrypoint fixture, not weights\n' > "$FIXTURE"
GOOD_SHA="$(shasum -a 256 "$FIXTURE" | awk '{print $1}')"
WRONG_SHA="$(printf '%064d' 0)"
HASH64="$(printf 'ab%.0s' $(seq 1 32))"
HASH63="${HASH64%?}"
HASH_G="g${HASH64#?}"
HASH_MULTI="$(printf '%s\njunk' "$HASH64")"
IMAGE_NEEDLE='[entry] REFUSED auth'
AWS_LOG="$TMP/aws.argv"
MODEL="$WORK/model.gguf"
OUT="$TMP/out"
ERR="$TMP/err"

PASS=0
FAILS=0
RC=0
WHY=""

# run_entry [VAR=value ...]: run the entrypoint under a clean env; sets RC, writes $OUT and $ERR.
run_entry() {
  rm -f "$MODEL" "$AWS_LOG"
  : > "$AWS_LOG"
  RC=0
  env -i PATH="$SHIMS:/usr/bin:/bin" APR_BIN="$SHIMS/apr" SHA256SUM="$SHIMS/sha256sum" \
    MODEL_LOCAL="$MODEL" PORT=8080 SHIM_AWS_LOG="$AWS_LOG" SHIM_FIXTURE="$FIXTURE" "$@" \
    bash "$ENTRY" > "$OUT" 2> "$ERR" || RC=$?
}

# expectations append to WHY; finish N prints PASS/FAIL for the row
expect_rc() {
  if [ "$RC" -ne "$1" ]; then WHY="$WHY exit=$RC want $1;"; fi
}
expect_in() { grep -qF -- "$2" "$1" || WHY="$WHY missing '$2' in $(basename "$1");"; }
expect_not_in() { if grep -qF -- "$2" "$1"; then WHY="$WHY unexpected '$2' in $(basename "$1");"; fi; }
finish() {
  if [ -z "$WHY" ]; then
    echo "PASS $1 $2"
    PASS=$((PASS + 1))
  else
    echo "FAIL $1 $2:$WHY"
    FAILS=$((FAILS + 1))
  fi
  WHY=""
}

# 1. hash unset
run_entry MODEL_SHA256="$GOOD_SHA" MODEL_S3_URI=s3://b/k
expect_rc 2; expect_in "$ERR" "REFUSED auth"; expect_not_in "$OUT" "EXEC OK"
finish 1 "hash unset -> REFUSED auth"

# 2. 63 hex
run_entry APR_API_KEY_HASH="$HASH63" MODEL_SHA256="$GOOD_SHA" MODEL_S3_URI=s3://b/k
expect_rc 2; expect_in "$ERR" "REFUSED auth"; expect_not_in "$OUT" "EXEC OK"
finish 2 "63 hex -> REFUSED auth"

# 3. 64 chars with a g
run_entry APR_API_KEY_HASH="$HASH_G" MODEL_SHA256="$GOOD_SHA" MODEL_S3_URI=s3://b/k
expect_rc 2; expect_in "$ERR" "REFUSED auth"; expect_not_in "$OUT" "EXEC OK"
# ...and a 64-hex line followed by a junk line: grep's per-line `^...$` matches the first line, so only
# the length glob refuses it (each guard has to be able to fail on its own).
run_entry APR_API_KEY_HASH="$HASH_MULTI" MODEL_SHA256="$GOOD_SHA" MODEL_S3_URI=s3://b/k
expect_rc 2; expect_in "$ERR" "REFUSED auth"; expect_not_in "$OUT" "EXEC OK"
finish 3 "64 chars with g, and 64 hex + junk line -> REFUSED auth"

# 4. valid hash, MODEL_SHA256 unset
run_entry APR_API_KEY_HASH="$HASH64" MODEL_S3_URI=s3://b/k
expect_rc 2; expect_in "$ERR" "REFUSED model-pin"; expect_not_in "$OUT" "EXEC OK"
finish 4 "MODEL_SHA256 unset -> REFUSED model-pin"

# 5. positive control: valid hash + correct sha, model absent, pulled through the aws shim
run_entry APR_API_KEY_HASH="$HASH64" MODEL_SHA256="$GOOD_SHA" MODEL_S3_URI=s3://b/k
expect_rc 0; expect_in "$OUT" "EXEC OK"; expect_in "$OUT" "serve run"; expect_in "$OUT" "--backend cpu"
expect_in "$AWS_LOG" "s3 cp"; expect_in "$OUT" "model sha256 OK"
expect_not_in "$OUT" "$HASH64"; expect_not_in "$ERR" "$HASH64"
finish 5 "valid hash + sha, pulled -> exec serve run (hash never logged)"

# 6. wrong sha: refuse, and the pulled file must be removed
run_entry APR_API_KEY_HASH="$HASH64" MODEL_SHA256="$WRONG_SHA" MODEL_S3_URI=s3://b/k
expect_rc 4; expect_in "$ERR" "REFUSED model-sha"; expect_not_in "$OUT" "EXEC OK"
[ ! -e "$MODEL" ] || WHY="$WHY pulled file still present;"
finish 6 "wrong MODEL_SHA256 -> REFUSED model-sha, file removed"

# 7. model already present with the right sha: no pull
rm -f "$MODEL" "$AWS_LOG"
cp "$FIXTURE" "$MODEL"
RC=0
env -i PATH="$SHIMS:/usr/bin:/bin" APR_BIN="$SHIMS/apr" SHA256SUM="$SHIMS/sha256sum" \
  MODEL_LOCAL="$MODEL" PORT=8080 SHIM_AWS_LOG="$AWS_LOG" SHIM_FIXTURE="$FIXTURE" \
  APR_API_KEY_HASH="$HASH64" MODEL_SHA256="$GOOD_SHA" \
  bash "$ENTRY" > "$OUT" 2> "$ERR" || RC=$?
expect_rc 0; expect_in "$OUT" "model already present"; expect_in "$OUT" "EXEC OK"
[ ! -s "$AWS_LOG" ] || WHY="$WHY aws shim was called;"
finish 7 "model already present, right sha -> exec, aws not called"

# 8. model absent and MODEL_S3_URI unset
run_entry APR_API_KEY_HASH="$HASH64" MODEL_SHA256="$GOOD_SHA"
expect_rc 2; expect_in "$OUT" "FATAL: MODEL_S3_URI unset"; expect_not_in "$OUT" "EXEC OK"
finish 8 "model absent, MODEL_S3_URI unset -> FATAL"

if [ "$FAILS" -ne 0 ] || [ "$PASS" -ne 8 ]; then
  echo "ENTRYPOINT SELFTEST FAILED $PASS/8 ($FAILS failing)"
  exit 1
fi
echo "ENTRYPOINT SELFTEST OK 8/8"

# The auth refusal inside the REAL image: it needs no model and no AWS, only the image.
if [ "${SKIP_IMAGE:-0}" = "1" ]; then
  echo "IMAGE PROOF SKIPPED (SKIP_IMAGE=1)"
  exit 0
fi
# shellcheck disable=SC1091
. "$HERE/pin.env"
IMAGE="$IMAGE_NAME:cpu-$APR_TAG"
command -v docker > /dev/null || { echo "FAIL: docker not found (SKIP_IMAGE=1 to run the table only)" >&2; exit 1; }
docker image inspect "$IMAGE" > /dev/null 2>&1 \
  || { echo "FAIL: image $IMAGE not built (run: just llm-endpoint-image)" >&2; exit 1; }
RC=0
docker run --rm -e APR_API_KEY_HASH=short -e MODEL_SHA256="$GOOD_SHA" "$IMAGE" > "$OUT" 2> "$ERR" || RC=$?
if [ "$RC" -ne 2 ] || ! grep -qF "$IMAGE_NEEDLE" "$ERR"; then
  echo "FAIL: image $IMAGE with a malformed hash exited $RC (want 2) / stderr: $(tail -3 "$ERR")" >&2
  exit 1
fi
echo "IMAGE PROOF OK $IMAGE refused a malformed APR_API_KEY_HASH (exit 2)"
