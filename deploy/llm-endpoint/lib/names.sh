#!/usr/bin/env bash
# names.sh: the one place the llm-endpoint name rule lives.
# Sourced:  . deploy/llm-endpoint/lib/names.sh || exit 1
# Executed: bash deploy/llm-endpoint/lib/names.sh --self-test
# A sourced library is option-neutral: no `set` at file scope, failure by return status.

# llm_endpoint_variant_ok VARIANT: return 0 if empty or a legal variant, 1 otherwise.
# 1 to 14 chars of [a-z0-9-], no leading or trailing hyphen (the S3 bucket name is the binding limit),
# and not ending in the segment `codebuild`.
llm_endpoint_variant_ok() {
    local LC_ALL=C
    local v="${1-}"
    [ -z "$v" ] && return 0
    # same rule as ^[a-z0-9]([a-z0-9-]{0,12}[a-z0-9])?$, written as globs
    case "$v" in
        *[!a-z0-9-]* | -* | *-) return 1 ;;
        # <env>-<variant>-codebuild is the CodeBuild role of <env>-<variant>, so a variant that
        # ends in it would be another name's function with the same IAM role name
        codebuild | *-codebuild) return 1 ;;
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
        SUFFIX="$env-$variant"
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

# llm_endpoint_check_record FILE: does this ids record belong to the function FN? Reads the globals
# FN, VARIANT and ENV the recipe set. A record with a `name` key (or, in function.json, a `function` key) must equal FN. A record without one
# is legacy (written before variants) and serves only the default variant, in the same env.
# Returns 0 to accept; otherwise 1 with a one-line reason in LLM_ENDPOINT_NAMES_ERR.
llm_endpoint_check_record() {
    local file="${1-}"
    local rc=0
    # one program line: bashrs mis-parses a python body inside the command substitution itself
    local prog='import json,sys;p,fn,v,e=sys.argv[1:5];r=json.load(open(p));r=r if isinstance(r,dict) else {};n=r.get("name",r.get("function"));m=("%s belongs to %s, not %s"%(p,n,fn) if n!=fn else "") if n is not None else ("%s has no name key (a record from before variants) and cannot serve variant %s"%(p,v) if v else ("%s has no name key and its env is %s, not %s"%(p,r.get("env"),e) if r.get("env")!=e else ""));print(m);sys.exit(1 if m else 0)'
    LLM_ENDPOINT_NAMES_ERR="$(python3 -I -c "$prog" "$file" "${FN-}" "${VARIANT-}" "${ENV-}" 2>&1)"
    rc=$?
    if [ "$rc" -ne 0 ]; then
        LLM_ENDPOINT_NAMES_ERR="${LLM_ENDPOINT_NAMES_ERR//$'\n'/ }"
        [ -n "$LLM_ENDPOINT_NAMES_ERR" ] || LLM_ENDPOINT_NAMES_ERR="$file could not be checked"
        return 1
    fi
    LLM_ENDPOINT_NAMES_ERR=""
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
    eq "2b FN" "$FN" aprender-llm-endpoint-dev-2b
    eq "2b REPO_NAME" "$REPO_NAME" aprender-llm-endpoint-dev-2b
    eq "2b SUFFIX" "$SUFFIX" dev-2b
    eq "2b BUCKET" "$BUCKET" aprender-llm-endpoint-weights-816069129666-dev-2b
    eq "2b CB_PROJECT" "$CB_PROJECT" aprender-llm-endpoint-weights-dev-2b
    eq "2b CB_ROLE" "$CB_ROLE" aprender-llm-endpoint-dev-2b-codebuild
    eq "2b LOG_GROUP" "$LOG_GROUP" /aws/lambda/aprender-llm-endpoint-dev-2b
    eq "2b IDS_DEFAULT" "$IDS_DEFAULT" deploy/llm-endpoint/.ids/dev-2b
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
    llm_endpoint_names 4b dev; a="$FN $CB_ROLE"
    llm_endpoint_names 2b dev; b="$FN $CB_ROLE"
    llm_endpoint_names "" dev; c="$FN $CB_ROLE"
    disjoint() {
        local x=""
        local y=""
        [ -n "$1" ] && [ -n "$2" ] || return 2
        for x in $1; do for y in $2; do [ "$x" != "$y" ] || return 1; done; done
        return 0
    }
    row "disjoint 4b vs 2b" 0 disjoint "$a" "$b"
    row "disjoint 4b vs empty" 0 disjoint "$a" "$c"
    row "disjoint 2b vs empty" 0 disjoint "$b" "$c"
    row "refuse variant codebuild (its function role is the CodeBuild role of the default)" 1 llm_endpoint_variant_ok codebuild
    row "refuse variant 2b-codebuild (its function role would be the 2b CodeBuild role)" 1 llm_endpoint_variant_ok 2b-codebuild
    row "accept variant codebuild2 (not the segment)" 0 llm_endpoint_variant_ok codebuild2
    row "accept variant codebuild-2b" 0 llm_endpoint_variant_ok codebuild-2b
    row "disjoint control: 2b vs 2b collides" 1 disjoint "$b" "$b"
    # longest legal bucket
    llm_endpoint_names a1b2c3d4e5f6g7 prod 816069129666
    eq "longest bucket length" "${#BUCKET}" 62
    # check_record: a record names its function; a record without a name is legacy and serves only the default variant
    local rd
    rd="${TMPDIR:-/tmp}/names-selftest.$$"
    mkdir -p "$rd" || return 1
    printf '{"name":"aprender-llm-endpoint-dev-2b","env":"dev"}\n' > "$rd/named.json"
    printf '{"env":"dev"}\n' > "$rd/legacy.json"
    printf '{"env":"prod"}\n' > "$rd/legacy-prod.json"
    printf '{"name":"aprender-llm-endpoint-dev","env":"dev"}\n' > "$rd/named-default.json"
    printf '{"function":"aprender-llm-endpoint-dev-2b","env":"dev","variant":"2b"}\n' > "$rd/function-2b.json"
    printf 'not json\n' > "$rd/garbage.json"
    FN=aprender-llm-endpoint-dev-2b VARIANT=2b ENV=dev
    row "record: matching name accepted" 0 llm_endpoint_check_record "$rd/named.json"
    FN=aprender-llm-endpoint-dev-4b VARIANT=4b ENV=dev
    row "record: mismatched name refused" 1 llm_endpoint_check_record "$rd/named.json"
    llm_endpoint_check_record "$rd/named.json"
    eq "record: mismatch reason names both" "$LLM_ENDPOINT_NAMES_ERR" "$rd/named.json belongs to aprender-llm-endpoint-dev-2b, not aprender-llm-endpoint-dev-4b"
    FN=aprender-llm-endpoint-dev VARIANT="" ENV=dev
    row "record: legacy accepted for the default variant" 0 llm_endpoint_check_record "$rd/legacy.json"
    row "record: named default accepted" 0 llm_endpoint_check_record "$rd/named-default.json"
    row "record: 2b-named record refused for the default" 1 llm_endpoint_check_record "$rd/named.json"
    FN=aprender-llm-endpoint-dev-2b VARIANT=2b ENV=dev
    row "record: legacy refused for a variant" 1 llm_endpoint_check_record "$rd/legacy.json"
    row "record: function.json keyed by function accepted for its variant" 0 llm_endpoint_check_record "$rd/function-2b.json"
    FN=aprender-llm-endpoint-dev VARIANT="" ENV=dev
    row "record: function.json of 2b refused for the default" 1 llm_endpoint_check_record "$rd/function-2b.json"
    FN=aprender-llm-endpoint-dev-2b VARIANT=2b ENV=dev
    FN=aprender-llm-endpoint-dev VARIANT="" ENV=dev
    row "record: legacy with the wrong env refused" 1 llm_endpoint_check_record "$rd/legacy-prod.json"
    row "record: unreadable file refused" 1 llm_endpoint_check_record "$rd/garbage.json"
    row "record: missing file refused" 1 llm_endpoint_check_record "$rd/absent.json"
    rm -rf "${rd:?}" || true
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
