#!/usr/bin/env bash
# Compare the same workload compiled against the base and proposed library.
# Usage: bash benches/check-regressions.sh <base-binary> <head-binary>
# Or: --measure <binary> emits counts; --compare <base-counts> <head-counts> checks them.
set -euo pipefail
mode=${1:-}
[[ $mode != --compare ]] || shift
[[ $# == 2 ]] || { echo "usage: $0 <base-binary> <head-binary> | --measure <binary> | --compare <base-counts> <head-counts>" >&2; exit 2; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# The bench copies its own path, argv[0], to the heap, so the length of that path moves later
# blocks and with them whether the tessellator's meshes grow in place. One binary counted 5% more
# instructions on `events` from the checkout root than from `performance-base/`, so both
# binaries run from paths of the same length.
mkdir "$work/base" "$work/head"
if [[ $mode == --measure ]]; then
    cp "$2" "$work/base/frame_time"
elif [[ $mode != --compare ]]; then
    cp "$1" "$work/base/frame_time"
    cp "$2" "$work/head/frame_time"
fi
base_bin=$work/base/frame_time
head_bin=$work/head/frame_time
log=$work/log
# glibc raises its mmap threshold as large blocks are freed, which changes whether growing a
# vector copies it. That moved the tessellator's mesh growth by about 5% for unrelated changes,
# so the threshold is fixed and counts follow the code.
export MALLOC_MMAP_THRESHOLD_=131072
instructions() {
    local count
    if ! FRAME_TIME_FRAMES=$3 valgrind --tool=cachegrind --cache-sim=no --branch-sim=no \
        --cachegrind-out-file=/dev/null "$1" --regression "$2" > /dev/null 2> "$log"; then
        cat "$log" >&2
        return 1
    fi
    count=$(sed -n 's/.*I *refs: *//p' "$log" | tr -d ,)
    [[ $count =~ ^[0-9]+$ ]] || { cat "$log" >&2; exit 1; }
    echo "$count"
}
if [[ $mode != --measure ]]; then
    printf '| Scenario | Base instructions | Head instructions | Change | Tolerance |\n'
    printf '|---|---:|---:|---:|---:|\n'
fi
fail=0
for scenario in frame-1000 frame-100000 frame-1000000 heikin tick tick-studies append append-studies bar-limit studies drawings depth bubbles tpo-400 tpo-1600 trade-replay session-resample markers events comparisons-1 comparisons-2 comparisons-5; do
    # A replay already looks up 100,000 ids; one measured batch is enough.
    samples=20
    [[ $scenario != trade-replay ]] || samples=1
    if [[ $mode == --measure ]]; then
        base0=$(instructions "$base_bin" "$scenario" 0)
        base_measured=$(instructions "$base_bin" "$scenario" "$samples")
        printf '%s %s %s\n' "$scenario" "$base0" "$base_measured"
        continue
    elif [[ $mode == --compare ]]; then
        read -r base0 base_measured <<< "$(awk -v s="$scenario" '$1 == s { print $2, $3 }' "$1")"
        read -r head0 head_measured <<< "$(awk -v s="$scenario" '$1 == s { print $2, $3 }' "$2")"
        [[ $base0 =~ ^[0-9]+$ && $base_measured =~ ^[0-9]+$ && $head0 =~ ^[0-9]+$ && $head_measured =~ ^[0-9]+$ ]] || {
            echo "::error::Invalid instruction delta for $scenario" >&2; exit 1;
        }
    else
        base0=$(instructions "$base_bin" "$scenario" 0)
        head0=$(instructions "$head_bin" "$scenario" 0)
        base_measured=$(instructions "$base_bin" "$scenario" "$samples")
        head_measured=$(instructions "$head_bin" "$scenario" "$samples")
    fi
    base=$(( base_measured - base0 ))
    head=$(( head_measured - head0 ))
    (( base > 0 && head > 0 )) || { echo "::error::Invalid instruction delta for $scenario" >&2; exit 1; }
    change=$(awk -v b="$base" -v h="$head" 'BEGIN { printf "%+.2f%%", (h-b)*100/b }')
    printf '| %s | %s | %s | %s | +5%% |\n' "$scenario" "$(( base / samples ))" "$(( head / samples ))" "$change"
    # Compare unrounded counts so a value just above the threshold still fails.
    if (( head * 100 > base * 105 )); then
        echo "::error title=Performance regression::$scenario grew by $change (tolerance +5%)" >&2
        fail=1
    fi
    if [[ $scenario == tpo-400 ]]; then
        tpo400=$head
    elif [[ $scenario == tpo-1600 ]] && (( head * 100 > tpo400 * 4 * 105 )); then
        echo "::error title=TPO scaling regression::Instructions per session grew by more than 5% from 400 to 1,600 sessions" >&2
        fail=1
    fi
done
exit "$fail"
