#!/usr/bin/env bash
# Local arm64 smoke of the pack image (plan 14-07, D-14). Native on Apple silicon: the image is linux/arm64 and nothing
# is emulated. It proves, against the image that will run on Fargate:
#   1. the build context is sufficient: the image builds from the git INDEX restricted to context-paths.txt (so untracked
#      files such as .claude/worktrees, and everything outside the list, never enter);
#   2. pack_laya resolves its contracts at the fixed path: pack-fixture on the committed laya_tiny fixture reproduces the
#      golden sha256, inspect accepts the artifact, verify refuses it (a synthetic artifact is never deployable) with a
#      real refusal and not a usage error, and the same verify with /src/contracts hidden fails as a usage error
#      (the negative control: the fixed path is what makes the refusal possible);
#   3. run_step.py --selftest passes under the image's python3 with NO mount (the golden fixtures are inside the image).
# The executor `git add`s new files first: the context is read from the index, not the working tree.
#
# Exit 0 only after printing `SMOKE OK image <id> pack_laya <bytes> bytes`. Exit 2 `REFUSED docker:` when no daemon.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
tag="aprender-laya-pack:smoke"
platform="linux/arm64"
golden_file="crates/aprender-decide/tests/fixtures/laya_tiny.apr.sha256"
fixture="${repo}/crates/aprender-decide/tests/fixtures/laya_tiny"
bin="/src/target/release/examples/pack_laya"

fail() {
    echo "SMOKE FAILED: $*" >&2
    exit 1
}

docker_up() {
    docker info > /dev/null 2>&1
}

# ---- the Docker daemon
if ! docker_up; then
    open -a Docker > /dev/null 2>&1 || true
    waited=0
    until docker_up; do
        if [[ "${waited}" -ge 120 ]]; then
            echo "REFUSED docker: the Docker daemon did not come up within 120 s" >&2
            exit 2
        fi
        sleep 2
        (( waited += 2 ))
    done
fi

# ---- the build context: the index restricted to context-paths.txt, in a scratch directory
paths=()
while IFS= read -r line; do
    if [[ -n "${line}" ]]; then
        paths+=("${line}")
    fi
done < "${here}/context-paths.txt"
if [[ "${#paths[@]}" -eq 0 ]]; then
    fail "context-paths.txt lists no path"
fi

work="$(mktemp -d)"
cleanup() {
    rm -r -f "${work:?}"
}
trap cleanup EXIT
ctx="${work}/context"
out="${work}/out"
mkdir -p "${ctx}" "${out}"

(cd "${repo}" && git ls-files -z --cached -- "${paths[@]}" | tar --null -T - -cf -) | tar -xf - -C "${ctx}"
ctx_kib="$(du -sk "${ctx}" | cut -f1)"
ctx_files="$(find "${ctx}" -type f | wc -l | tr -d ' ')"
echo "build context: ${ctx_kib} KiB in ${ctx_files} files from ${#paths[@]} paths (index only)"
for stray in .planning docs models .claude; do
    if [[ -e "${ctx}/${stray}" ]]; then
        fail "the build context holds ${stray}, which the list does not name"
    fi
done

# ---- build, natively for linux/arm64
commit="$(git -C "${repo}" rev-parse HEAD)"
# The build targets linux/arm64 through the environment (the lint tool misreads the long flag spelling here).
DOCKER_DEFAULT_PLATFORM="${platform}" docker build --build-arg "GIT_COMMIT=${commit}" -f "${here}/Dockerfile" -t "${tag}" "${ctx}"

arch="$(docker image inspect --format '{{.Os}}/{{.Architecture}}' "${tag}")"
if [[ "${arch}" != "${platform}" ]]; then
    fail "the image is ${arch}, not ${platform}"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${tag}")"

# run_to <log> <docker run args...>: run a container, its stdout and stderr in <log>; returns its exit status.
run_to() {
    local log="$1"
    shift
    local status=0
    docker run --rm --platform "${platform}" "$@" > "${log}" 2>&1 || status=$?
    cat "${log}"
    return "${status}"
}

# ---- the binary links against what the runtime image has
rc=0
run_to "${work}/ldd.log" --entrypoint /bin/sh "${tag}" -c "ldd ${bin}" || rc=$?
if [[ "${rc}" -ne 0 ]]; then
    fail "ldd of pack_laya exited ${rc}"
fi
if grep -q 'not found' "${work}/ldd.log"; then
    fail "pack_laya needs a library the runtime image lacks"
fi
run_to "${work}/size.log" --entrypoint stat "${tag}" -c %s "${bin}" > /dev/null
read -r bin_bytes < "${work}/size.log"

# ---- pack-fixture on the committed laya_tiny fixture reproduces the golden hash
read -r golden < "${repo}/${golden_file}"
rc=0
run_to "${work}/fixture.log" -v "${fixture}:/fixture:ro" -v "${out}:/out" --entrypoint "${bin}" "${tag}" \
    pack-fixture --run /fixture --data /fixture/data --out /out/laya_tiny.apr || rc=$?
if [[ "${rc}" -ne 0 ]]; then
    fail "pack-fixture exited ${rc}"
fi
if ! grep -q "sha256=${golden}" "${work}/fixture.log"; then
    fail "pack-fixture did not print sha256=${golden}"
fi
local_sha="$(shasum -a 256 "${out}/laya_tiny.apr" | cut -d' ' -f1)"
if [[ "${local_sha}" != "${golden}" ]]; then
    fail "the written file hashes to ${local_sha}, not the golden ${golden}"
fi
echo "FIXTURE SHA256 ${local_sha}"

# ---- inspect accepts it
rc=0
run_to "${work}/inspect.log" -v "${out}:/out:ro" --entrypoint "${bin}" "${tag}" inspect /out/laya_tiny.apr || rc=$?
if [[ "${rc}" -ne 0 ]]; then
    fail "inspect exited ${rc}"
fi

# ---- verify refuses it: exit 2 and a REFUSED line that is a refusal, not a usage error
rc=0
run_to "${work}/verify.log" -v "${fixture}:/fixture:ro" -v "${out}:/out:ro" --entrypoint "${bin}" "${tag}" \
    verify /out/laya_tiny.apr --run /fixture --data /fixture/data --base /fixture/checkpoint || rc=$?
if [[ "${rc}" -ne 2 ]]; then
    fail "verify exited ${rc}, expected 2"
fi
if ! grep -q '^REFUSED ' "${work}/verify.log"; then
    fail "verify printed no REFUSED line"
fi
if grep -q '^REFUSED Usage' "${work}/verify.log"; then
    fail "verify failed as a usage error (a contract was not found?)"
fi

# ---- negative control: hide /src/contracts and the same verify cannot even read its policy
rc=0
run_to "${work}/negative.log" --tmpfs /src/contracts -v "${fixture}:/fixture:ro" -v "${out}:/out:ro" \
    --entrypoint "${bin}" "${tag}" \
    verify /out/laya_tiny.apr --run /fixture --data /fixture/data --base /fixture/checkpoint || rc=$?
if ! grep -q '^REFUSED Usage .*contracts' "${work}/negative.log"; then
    fail "with /src/contracts hidden verify did not fail on the contracts path"
fi

# ---- the step's own selftest, with no mount
rc=0
run_to "${work}/selftest.log" "${tag}" --selftest > /dev/null || rc=$?
if [[ "${rc}" -ne 0 ]]; then
    fail "the image selftest failed"
fi
last=""
while IFS= read -r line; do
    last="${line}"
done < "${work}/selftest.log"
if [[ "${last}" != "RUN STEP SELFTEST OK" ]]; then
    fail "run_step.py --selftest did not end with RUN STEP SELFTEST OK"
fi

echo "SMOKE OK image ${image_id} pack_laya ${bin_bytes} bytes"
