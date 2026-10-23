#!/usr/bin/env bash
# Bearer-auth table for `apr serve` started with APR_API_KEY (contract rows 002 and 013).
#
# bash, not zsh: word splitting matters. Every status is captured with
# `code=$(curl ... -w '%{http_code}')`, never `$?` through a pipe.
#
#   auth_probe.sh [--expect gated|open] [PORT [KEY]]
#
#   PORT  defaults to 8032. HOST (env) defaults to 127.0.0.1.
#   KEY   defaults to the APR_PROBE_KEY environment variable. It is sent as a Bearer header
#         and is never printed.
#
# Without --expect the table is printed and the script exits 0. With --expect it also ASSERTS:
#   gated  the no-header and wrong-key rows are 401 AND the right-key row is 200 AND /health is
#          401 AND /v1/models is 401 (the D-04 declaration: every route is gated, /health
#          included, FALSIFY-AUTH-001). Needs a key.
#   open   every row is 200 (the release-pin baseline, asserted rather than eyeballed).
# Exit status: 0 pass, 1 an assertion failed, 2 usage error.
set -euo pipefail

usage() {
    printf 'usage: %s [--expect gated|open] [PORT [KEY]]\n' "${0##*/}" >&2
    exit 2
}

EXPECT=""
POSITIONAL=()
while [ "$#" -gt 0 ]; do
    case "$1" in
        --expect)
            [ "$#" -ge 2 ] || usage
            EXPECT="$2"
            shift 2
            ;;
        --expect=*)
            EXPECT="${1#--expect=}"
            shift
            ;;
        -h | --help)
            usage
            ;;
        *)
            POSITIONAL+=("$1")
            shift
            ;;
    esac
done
case "$EXPECT" in
    "" | gated | open) ;;
    *) usage ;;
esac
[ "${#POSITIONAL[@]}" -le 2 ] || usage

PORT="${POSITIONAL[0]:-8032}"
KEY="${POSITIONAL[1]:-}"
if [ -z "$KEY" ]; then
    KEY="${APR_PROBE_KEY:-}"
fi
HOST="${HOST:-127.0.0.1}"
BASE="http://${HOST}:${PORT}"

if [ "$EXPECT" = "gated" ] && [ -z "$KEY" ]; then
    printf 'error: --expect gated needs a key (second argument or APR_PROBE_KEY)\n' >&2
    exit 2
fi

REQ='{"model":"x","messages":[{"role":"user","content":"Hi"}],"max_tokens":4}'
NL=$'\n'

# split_out OUT: OUT is curl's body, a newline, then the status written by -w.
# Sets CODE (the status) and SNIPPET (the first 120 characters of the body, on one line).
CODE=""
SNIPPET=""
split_out() {
    local out="$1"
    local body="${out%"$NL"*}"
    CODE="${out##*"$NL"}"
    if [ -z "$CODE" ]; then
        CODE="000"
    fi
    body="${body//"$NL"/ }"
    body="${body//|//}"
    SNIPPET="${body:0:120}"
}

# post_row LABEL [BEARER]: sets CODE and prints one table row. The bearer goes to curl as a header
# FILE on stdin (`-H @-`, printf is a builtin), never as an argument: an argv is visible to every
# local user through `ps`.
post_row() {
    local label="$1"
    local bearer="${2-}"
    local out=""
    if [ -n "$bearer" ]; then
        out=$(printf 'Authorization: Bearer %s\n' "$bearer" \
            | curl -s -m 180 -w '\n%{http_code}' -H 'Content-Type: application/json' -H @- -d "$REQ" "${BASE}/v1/chat/completions") || true
    else
        out=$(curl -s -m 180 -w '\n%{http_code}' -H 'Content-Type: application/json' -d "$REQ" "${BASE}/v1/chat/completions") || true
    fi
    split_out "$out"
    printf '| %s | %s | %s |\n' "$label" "$CODE" "$SNIPPET"
}

# get_row LABEL PATH [curl header args...]: sets CODE and prints one table row.
get_row() {
    local label="$1"
    local path="$2"
    shift 2
    local out=""
    out=$(curl -s -m 10 -w '\n%{http_code}' "$@" "${BASE}${path}") || true
    split_out "$out"
    printf '| %s | %s | %s |\n' "$label" "$CODE" "$SNIPPET"
}

printf '| request | HTTP | body |\n|---|---|---|\n'
post_row "POST /v1/chat/completions, no Authorization header"
C_NOHDR="$CODE"
post_row "POST /v1/chat/completions, Bearer wrong-key" "wrong-key"
C_WRONG="$CODE"
C_RIGHT=""
if [ -n "$KEY" ]; then
    post_row "POST /v1/chat/completions, Bearer <the key>" "$KEY"
    C_RIGHT="$CODE"
fi
get_row "GET /health, no header" "/health"
C_HEALTH="$CODE"
get_row "GET /v1/models, no header" "/v1/models"
C_MODELS="$CODE"

fail() {
    printf 'FAIL (--expect %s): %s\n' "$EXPECT" "$1" >&2
    exit 1
}

case "$EXPECT" in
    gated)
        [ "$C_NOHDR" = "401" ] || fail "no-header POST is ${C_NOHDR}, want 401"
        [ "$C_WRONG" = "401" ] || fail "wrong-key POST is ${C_WRONG}, want 401"
        [ "$C_RIGHT" = "200" ] || fail "right-key POST is ${C_RIGHT}, want 200"
        [ "$C_HEALTH" = "401" ] || fail "GET /health is ${C_HEALTH}, want 401, health is not exempt per FALSIFY-AUTH-001"
        [ "$C_MODELS" = "401" ] || fail "GET /v1/models is ${C_MODELS}, want 401"
        printf 'PASS: gated (401 without a valid bearer on every row, 200 with the key)\n'
        ;;
    open)
        [ "$C_NOHDR" = "200" ] || fail "no-header POST is ${C_NOHDR}, want 200"
        [ "$C_WRONG" = "200" ] || fail "wrong-key POST is ${C_WRONG}, want 200"
        if [ -n "$C_RIGHT" ]; then
            [ "$C_RIGHT" = "200" ] || fail "right-key POST is ${C_RIGHT}, want 200"
        fi
        [ "$C_HEALTH" = "200" ] || fail "GET /health is ${C_HEALTH}, want 200"
        [ "$C_MODELS" = "200" ] || fail "GET /v1/models is ${C_MODELS}, want 200"
        printf 'PASS: open (every row is 200: no auth at the server)\n'
        ;;
esac
