#!/usr/bin/env bash
# names.sh: the one place the llm-endpoint name rule lives.
# Sourced:  . deploy/llm-endpoint/lib/names.sh || exit 1
# Executed: bash deploy/llm-endpoint/lib/names.sh --self-test
# A sourced library is option-neutral: no `set` at file scope, failure by return status.

# llm_endpoint_variant_ok VARIANT: return 0 if empty or a legal variant, 1 otherwise.
# 1 to 14 chars of [a-z0-9-], no leading or trailing hyphen (the S3 bucket name is the binding limit).
llm_endpoint_variant_ok() {
    local LC_ALL=C
    local v="${1-}"
    [ -z "$v" ] && return 0
    # same rule as ^[a-z0-9]([a-z0-9-]{0,12}[a-z0-9])?$, written as globs
    case "$v" in
        *[!a-z0-9-]* | -* | *-) return 1 ;;
    esac
    [[ ${#v} -le 14 ]]
}

# llm_endpoint_names VARIANT ENV [ACCOUNT]: set every derived name, or return 1 with
# LLM_ENDPOINT_NAMES_ERR set. BUCKET is set only when ACCOUNT is given.
llm_endpoint_names() {
    local variant="${1-}"
    local env="${2-}"
    local account="${3-}"
    LLM_ENDPOINT_NAMES_ERR=""
    if ! llm_endpoint_variant_ok "$variant"; then
        LLM_ENDPOINT_NAMES_ERR="variant '$variant' is not 1-14 chars of [a-z0-9-] without a leading or trailing hyphen"
        return 1
    fi
    case "$env" in
        dev | prod) ;;
        *)
            LLM_ENDPOINT_NAMES_ERR="env '$env' is not dev or prod"
            return 1
            ;;
    esac
    if [ -n "$variant" ]; then
        SUFFIX="$variant-$env"
        IDS_DEFAULT="deploy/llm-endpoint/.ids/$SUFFIX"
        PIN_DEFAULT="deploy/llm-endpoint/pin-$variant.env"
    else
        SUFFIX="$env"
        IDS_DEFAULT="deploy/llm-endpoint/.ids"
        PIN_DEFAULT="deploy/llm-endpoint/pin.env"
    fi
    FN="aprender-llm-endpoint-$SUFFIX"
    REPO_NAME="$FN"
    LOG_GROUP="/aws/lambda/$FN"
    CB_PROJECT="aprender-llm-endpoint-weights-$SUFFIX"
    CB_ROLE="$FN-codebuild"
    TAG_VARIANT="variant=$variant"
    BUCKET=""
    if [ -n "$account" ]; then
        BUCKET="aprender-llm-endpoint-weights-$account-$SUFFIX"
    fi
    return 0
}

# ---- self-test (case table first; the functions are defined above this block) ----
llm_endpoint_names_selftest() {
    local fails=0
    local passes=0
    pass() { passes=$((passes + 1)); echo "PASS $1"; }
    fail() { fails=$((fails + 1)); echo "FAIL $1"; }
    # row <label> <expected rc> <command...>: run the command, compare its return status
    row() {
        local label="$1"
        local want="$2"
        local rc=0
        shift 2
        "$@" > /dev/null 2>&1
        rc=$?
        if [ "$rc" -eq "$want" ]; then pass "$label"; else fail "$label: rc=$rc want $want"; fi
    }
    # eq <label> <actual> <expected>
    eq() {
        if [ "$2" = "$3" ]; then pass "$1"; else fail "$1: got '$2' want '$3'"; fi
    }
    local v
    for v in 2b 4b 2b-q4km x a1b2c3d4e5f6g7 ""; do
        row "accept variant '$v'" 0 llm_endpoint_variant_ok "$v"
    done
    for v in 2B 2_b -2b 2b- a1b2c3d4e5f6g7h 2b.q4 "2b dev" aprender-llm-endpoint-2b $'2b\nx'; do
        row "refuse variant '$v'" 1 llm_endpoint_variant_ok "$v"
    done
    row "refuse env staging" 1 llm_endpoint_names 2b staging
    row "refuse env empty" 1 llm_endpoint_names 2b ""
    row "refuse bad variant in names" 1 llm_endpoint_names 2B dev
    row "accept 2b dev" 0 llm_endpoint_names 2b dev
    # derived, 2b dev 816069129666
    llm_endpoint_names 2b dev 816069129666
    eq "2b FN" "$FN" aprender-llm-endpoint-2b-dev
    eq "2b REPO_NAME" "$REPO_NAME" aprender-llm-endpoint-2b-dev
    eq "2b SUFFIX" "$SUFFIX" 2b-dev
    eq "2b BUCKET" "$BUCKET" aprender-llm-endpoint-weights-816069129666-2b-dev
    eq "2b CB_PROJECT" "$CB_PROJECT" aprender-llm-endpoint-weights-2b-dev
    eq "2b CB_ROLE" "$CB_ROLE" aprender-llm-endpoint-2b-dev-codebuild
    eq "2b LOG_GROUP" "$LOG_GROUP" /aws/lambda/aprender-llm-endpoint-2b-dev
    eq "2b IDS_DEFAULT" "$IDS_DEFAULT" deploy/llm-endpoint/.ids/2b-dev
    eq "2b PIN_DEFAULT" "$PIN_DEFAULT" deploy/llm-endpoint/pin-2b.env
    eq "2b TAG_VARIANT" "$TAG_VARIANT" variant=2b
    # derived, empty dev
    llm_endpoint_names "" dev
    eq "empty FN" "$FN" aprender-llm-endpoint-dev
    eq "empty IDS_DEFAULT" "$IDS_DEFAULT" deploy/llm-endpoint/.ids
    eq "empty PIN_DEFAULT" "$PIN_DEFAULT" deploy/llm-endpoint/pin.env
    eq "empty CB_ROLE" "$CB_ROLE" aprender-llm-endpoint-dev-codebuild
    eq "empty TAG_VARIANT" "$TAG_VARIANT" variant=
    eq "empty BUCKET without account" "$BUCKET" ""
    # a stale BUCKET from an earlier call must not leak into a call with no account
    llm_endpoint_names 2b dev 816069129666
    llm_endpoint_names 2b dev
    eq "BUCKET reset when no account" "$BUCKET" ""
    # collisions: {FN, CB_ROLE} sets must be pairwise disjoint
    local a=""
    local b=""
    local c=""
    llm_endpoint_names codebuild-2b dev; a="$FN $CB_ROLE"
    llm_endpoint_names 2b dev; b="$FN $CB_ROLE"
    llm_endpoint_names "" dev; c="$FN $CB_ROLE"
    disjoint() {
        local x=""
        local y=""
        [ -n "$1" ] && [ -n "$2" ] || return 2
        for x in $1; do for y in $2; do [ "$x" != "$y" ] || return 1; done; done
        return 0
    }
    row "disjoint codebuild-2b vs 2b" 0 disjoint "$a" "$b"
    row "disjoint codebuild-2b vs empty" 0 disjoint "$a" "$c"
    llm_endpoint_names codebuild dev; a="$FN $CB_ROLE"
    row "disjoint codebuild vs empty" 0 disjoint "$a" "$c"
    row "disjoint control: 2b vs 2b collides" 1 disjoint "$b" "$b"
    # longest legal bucket
    llm_endpoint_names a1b2c3d4e5f6g7 prod 816069129666
    eq "longest bucket length" "${#BUCKET}" 62
    echo "names.sh self-test: $passes passed, $fails failed"
    (( fails == 0 ))
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    case "${1:-}" in
        --self-test)
            llm_endpoint_names_selftest
            exit $?
            ;;
        *)
            echo "usage: names.sh --self-test (or source it)" >&2
            exit 2
            ;;
    esac
fi
