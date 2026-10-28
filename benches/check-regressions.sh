#!/usr/bin/env bash
# Compare the same workload compiled against the base and proposed library on one runner.
# Usage: bash benches/check-regressions.sh <base-binary> <head-binary>
set -euo pipefail
[[ $# == 2 ]] || { echo "usage: $0 <base-binary> <head-binary>" >&2; exit 2; }
base_bin=$(realpath "$1")
head_bin=$(realpath "$2")
log=$(mktemp)
trap 'rm -f "$log"' EXIT
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
printf '| Scenario | Base instructions | Head instructions | Change | Tolerance |\n'
printf '|---|---:|---:|---:|---:|\n'
fail=0
for scenario in frame-1000 frame-100000 frame-1000000 heikin tick tick-studies append append-studies studies drawings; do
    base0=$(instructions "$base_bin" "$scenario" 0)
    head0=$(instructions "$head_bin" "$scenario" 0)
    base20=$(instructions "$base_bin" "$scenario" 20)
    head20=$(instructions "$head_bin" "$scenario" 20)
    base=$(( base20 - base0 ))
    head=$(( head20 - head0 ))
    (( base > 0 && head > 0 )) || { echo "::error::Invalid instruction delta for $scenario" >&2; exit 1; }
    change=$(awk -v b="$base" -v h="$head" 'BEGIN { printf "%+.2f%%", (h-b)*100/b }')
    printf '| %s | %s | %s | %s | +5%% |\n' "$scenario" "$(( base / 20 ))" "$(( head / 20 ))" "$change"
    # Compare unrounded counts so a value just above the threshold still fails.
    if (( head * 100 > base * 105 )); then
        echo "::error title=Performance regression::$scenario grew by $change (tolerance +5%)" >&2
        fail=1
    fi
done
exit "$fail"
