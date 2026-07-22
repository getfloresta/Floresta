#!/bin/bash

# SPDX-License-Identifier: MIT OR Apache-2.

# Checks each commit between BASE and HEAD by rebasing and running
# build, lint, unit, and functional tests per commit.
# Writes per-commit logs to /tmp/floresta-commits-logs/<sha>/*.log
# and overall results to /tmp/floresta-commits-results.log; exits non-zero on failures.
# Usage: contrib/check_each_commit.sh [BASE] [FLAGS]  # use --help for more info

set -uo pipefail

BASE="origin/master"
SKIP_FUNCTIONAL=false
SKIP_LINT=false
SKIP_UNIT_TESTS=false
RUN_CHECKS_MODE=false
REBASE_ABORTED=false
INTERRUPTED=false
RESULTS_FILE="/tmp/floresta-commits-results.log"
LOG_DIR="/tmp/floresta-commits-logs"
CURRENT_HASH=""

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --run-checks) RUN_CHECKS_MODE=true ;;
            --skip-functional) SKIP_FUNCTIONAL=true ;;
            --skip-lint) SKIP_LINT=true ;;
            --skip-unit-tests) SKIP_UNIT_TESTS=true ;;
            --help|-h) show_help; exit 0 ;;
            --*) echo "Unknown option: $1" >&2; exit 1 ;;
            *) BASE="$1" ;;
        esac
        shift
    done
}

show_help() {
    cat <<EOF
Usage: $0 [BASE] [OPTIONS]

Check each commit between BASE and HEAD individually.

Options:
  --run-checks          Run checks for the current commit (used internally during rebase)
  --skip-functional     Skip functional tests
  --skip-lint           Skip lint checks (includes functional tests lint)
  --skip-unit-tests     Skip unit tests
  -h, --help            Show this help message
EOF
}

cleanup() {
    if [ "$REBASE_ABORTED" = false ] && is_rebase_in_progress; then
        echo "Aborting rebase..." >&2
        git rebase --abort 2>/dev/null || true
        REBASE_ABORTED=true
    fi
}

handle_interrupt() {
    INTERRUPTED=true
    cleanup
}

trap cleanup EXIT
trap handle_interrupt INT TERM

print_pass() { echo "  [PASS] $1"; }
print_fail() { echo "  [FAIL] $1"; }
print_skip() { echo "  [SKIP] $1"; }
fmt_result() { case "$1" in true) echo -n pass ;; false) echo -n fail ;; *) echo -n skip ;; esac; }
sanitize_label() { echo "$1" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd '[:alnum:]_-'; }
resolve_project_root() { cd "$(git rev-parse --show-toplevel)" || return; }

is_rebase_in_progress() {
    local gitdir
    gitdir="$(git rev-parse --git-dir 2>/dev/null || true)"
    [ -n "$gitdir" ] && {
        [ -d "$gitdir/rebase-merge" ] || [ -d "$gitdir/rebase-apply" ]
    }
}

get_log_file_path() {
    mkdir -p "$LOG_DIR/$CURRENT_HASH"
    echo "$LOG_DIR/$CURRENT_HASH/$(sanitize_label "$1").log"
}

resolve_merge_base() {
    if ! git rev-parse --verify "$BASE" >/dev/null 2>&1; then
        echo "Error: base ref '$BASE' does not exist." >&2
        echo "Did you forget to 'git fetch origin'?" >&2
        exit 1
    fi
    local merge_base
    merge_base="$(git merge-base "$BASE" HEAD)"
    [ "$merge_base" = "$(git rev-parse HEAD)" ] && {
        echo "HEAD is at or behind $BASE - no commits to check."
        exit 0
    }
    echo "$merge_base"
}

run_check() {
    local label="$1" log_file exit_code
    shift
    echo "  Running: $label"
    log_file="$(get_log_file_path "$label")"
    "$@" > "$log_file" 2>&1
    exit_code=$?
    if [ "$exit_code" -gt 128 ]; then
        echo "  [INTERRUPTED] $label (signal $((exit_code - 128)))"
        echo "  Check log: $log_file"
        exit "$exit_code"
    fi
    [ "$exit_code" -eq 0 ] && { print_pass "$label"; return 0; }
    print_fail "$label"
    echo "  Check log: $log_file" >&2
    return 1
}

check_build() {
    run_check "build" cargo build --release --bins --verbose
}

check_lint() {
    run_check "lint" bash -c \
        "cargo +nightly fmt --all --check && \
        RUSTDOCFLAGS='--cfg docsrs -D warnings' cargo +nightly doc --workspace --no-deps \
        --all-features --lib --document-private-items --exclude metrics && \
        cargo +nightly clippy --workspace --all-targets --no-default-features -- -D warnings && \
        cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings"
}

check_unit_tests() {
    run_check "unit tests" bash -c \
        "cargo build && \
        cargo test --doc && \
        cargo test --lib -- --nocapture && \
        cargo test --workspace -- --nocapture"
}

check_functional_tests_lint() {
    run_check "functional tests lint" bash -c \
        "uv run black --check --verbose ./tests && \
        find ./tests -name '*.py' -print0 | xargs -0 uv run pylint --verbose"
}

check_functional_tests() {
    run_check "functional tests" bash -c \
        "tests/prepare.sh --release && tests/run.sh"
}

run_or_skip() {
    local key="$1" label="$2" skip_flag="$3" fn="$4" result
    if [ "$skip_flag" = true ]; then
        print_skip "$label"
        result=skipped
    elif "$fn"; then
        result=true
    else
        result=false
    fi
    RESULTS["$key"]="$result"
    [ "$result" != false ]
}

declare -A RESULTS

run_checks_mode() {
    resolve_project_root
    local sha short_sha msg overall=true
    sha="$(git rev-parse HEAD)"
    short_sha="$(git rev-parse --short "$sha")"
    msg="$(git log -1 --format='%s' "$sha")"
    CURRENT_HASH="$short_sha"

    echo "--- Commit $short_sha: $msg ---"

    run_or_skip build "build" false check_build || overall=false
    run_or_skip lint "lint" "$SKIP_LINT" check_lint || overall=false
    run_or_skip unit "unit tests" "$SKIP_UNIT_TESTS" check_unit_tests || overall=false
    run_or_skip func_lint "functional tests lint" "$SKIP_LINT" check_functional_tests_lint || overall=false
    run_or_skip func "functional tests" "$SKIP_FUNCTIONAL" check_functional_tests || overall=false

    echo "$short_sha|$msg|${RESULTS[build]}|${RESULTS[lint]}|"\
        "${RESULTS[unit]}|${RESULTS[func_lint]}|${RESULTS[func]}|$overall" \
        >> "$RESULTS_FILE"
    echo ""
    [ "$overall" = true ] && exit 0 || exit 1
}

print_summary() {
    local mode="${1:-normal}" expected_total="${2:-0}" total=0 failed=0
    echo ""
    echo "==============================================================="
    echo "Per-commit check summary"
    echo "==============================================================="
    echo ""
    printf "%-10s %-40s %-8s %-8s %-8s %-10s %-12s %s\n" "SHA" "Message" "Build" "Lint" "Unit" "FuncLint" "Functional" "Status"
    printf "%-10s %-40s %-8s %-8s %-8s %-10s %-12s %s\n" "----------" "----------------------------------------" "------" "------" "------" "--------" "----------" "------"

    if [ ! -s "$RESULTS_FILE" ]; then
        echo "ERROR: Per-commit check failed - no commits were tested."
        echo ""
        echo "Possible causes:"
        echo "  1. Your fork's master branch may be out of date"
        echo "  2. The branch needs to be rebased on the latest master branch"
        echo ""
        return 1
    fi

    while IFS='|' read -r sha msg build lint unit func_lint func overall; do
        total=$((total + 1))
        [ ${#msg} -gt 40 ] && msg="${msg:0:37}..."
        local status=PASS
        if [ "$overall" != true ]; then status=FAILED; failed=$((failed + 1)); fi
        printf "%-10s %-40s %-8s %-8s %-8s %-10s %-12s %s\n" \
            "$sha" "$msg" \
            "$(fmt_result "$build")" \
            "$(fmt_result "$lint")" \
            "$(fmt_result "$unit")" \
            "$(fmt_result "$func_lint")" \
            "$(fmt_result "$func")" \
            "$status"
    done < "$RESULTS_FILE"

    echo ""
    echo "Total: $total | Passed: $((total - failed)) | Failed: $failed"
    echo ""

    if [ "$mode" = interrupted ]; then
        echo "Per-commit check interrupted."
        [ "$expected_total" -gt 0 ] && \
            echo "Checked $total of $expected_total commit(s) before interruption."
        return 130
    fi

    [ "$failed" -eq 0 ] && { echo "All commits passed individual checks!"; return 0; }

    echo "Some commits failed individual checks."
    echo "Failed commits log paths:"
    while IFS='|' read -r sha _ build lint unit func_lint func overall; do
        [ "$overall" = false ] || continue
        local dir="$LOG_DIR/$sha"
        echo "  - commit $sha: $dir"
        [ "$build" = false ] && echo "      - build log: $dir/build.log"
        [ "$lint" = false ] && echo "      - lint log: $dir/lint.log"
        [ "$unit" = false ] && echo "      - unit tests log: $dir/unit-tests.log"
        [ "$func_lint" = false ] && \
            echo "      - functional tests lint log: $dir/functional-tests-lint.log"
        [ "$func" = false ] && echo "      - functional tests log: $dir/functional-tests.log"
    done < "$RESULTS_FILE"
    return 1
}

main() {
    parse_args "$@"
    if [ "$RUN_CHECKS_MODE" = true ]; then
        trap - EXIT INT TERM
        run_checks_mode
        return
    fi

    resolve_project_root

    local merge_base script_path rebase_rc exec_args="--run-checks"
    merge_base="$(resolve_merge_base)"
    mapfile -t commits < <(git rev-list --reverse "$merge_base..HEAD")

    echo "Checking ${#commits[@]} commit(s) individually"
    echo "Base: $BASE ($(git rev-parse --short "$merge_base"))"
    echo "Head: HEAD ($(git rev-parse --short HEAD))"
    echo ""

    : > "$RESULTS_FILE"
    [ "$SKIP_FUNCTIONAL" = true ] && exec_args="$exec_args --skip-functional"
    [ "$SKIP_LINT" = true ] && exec_args="$exec_args --skip-lint"
    [ "$SKIP_UNIT_TESTS" = true ] && exec_args="$exec_args --skip-unit-tests"
    script_path="$(realpath "$0")"

    git rebase --exec "$script_path $exec_args" "$merge_base"
    rebase_rc=$?

    if is_rebase_in_progress; then
        echo "Rebase stopped, aborting to restore original state..." >&2
        git rebase --abort 2>/dev/null || true
        REBASE_ABORTED=true
    fi

    if [ "$INTERRUPTED" = true ] || [ "$rebase_rc" -gt 128 ]; then
        print_summary interrupted "${#commits[@]}"
        return 130
    fi

    print_summary normal "${#commits[@]}"
}

main "$@"
