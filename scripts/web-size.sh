#!/usr/bin/env bash
# Builds the bare chart (`examples/bare_chart.rs` with `--no-default-features --features widget`)
# and the demo (`examples/demo`) for the web with `trunk build --release`, the way the demo is
# deployed, and prints the size in bytes of each one's module and JavaScript glue: raw, after
# `gzip -9` and after `brotli -q 11`. The bare chart takes the Trunk settings of the demo's page.
#
#     scripts/web-size.sh [bare] [demo]
#
# Needs the wasm32-unknown-unknown target, Trunk, gzip and brotli. The builds go to target/web-size.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out=$root/target/web-size
mkdir -p "$out"

# One row per file of the bundle in $2 (the module and the glue; not the page or the licences)
# and one for their sum.
measure() {
    local name=$1 dist=$2 kind file i sizes total=(0 0 0)
    for kind in wasm js; do
        file=$(find "$dist" -maxdepth 1 -name "*.$kind" | head -1)
        sizes=("$(stat -c %s "$file")" "$(gzip -9 -c "$file" | wc -c)" "$(brotli -q 11 -c "$file" | wc -c)")
        echo "| $name | $kind | ${sizes[0]} | ${sizes[1]} | ${sizes[2]} |"
        for i in 0 1 2; do total[i]=$((total[i] + sizes[i])); done
    done
    echo "| $name | total | ${total[0]} | ${total[1]} | ${total[2]} |"
}

build_bare() {
    local page=$out/bare/index.html
    mkdir -p "$out/bare"
    # The demo's Rust link, pointed at the library (from target/web-size/bare) and its widget feature.
    sed -n 's|.*<link data-trunk rel="rust"\(.*\)/>.*|<link data-trunk rel="rust" href="../../../Cargo.toml" data-cargo-no-default-features data-cargo-features="widget"\1/>|p' \
        "$root/examples/demo/index.html" > "$page"
    grep -q 'rel="rust"' "$page" || { echo "no Rust link in examples/demo/index.html" >&2; exit 1; }
    trunk build --release --example bare_chart --dist "$out/bare/dist" "$page" >&2
}

build_demo() {
    (cd "$root/examples/demo" && trunk build --release --dist "$out/demo/dist") >&2
}

echo '| Build | File | Raw | gzip -9 | brotli -q 11 |'
echo '|---|---|---:|---:|---:|'
for target in "${@:-bare demo}"; do
    for target in $target; do
        case $target in
            bare) build_bare; measure "Bare chart" "$out/bare/dist" ;;
            demo) build_demo; measure "Demo" "$out/demo/dist" ;;
            *) echo "unknown build: $target (bare or demo)" >&2; exit 2 ;;
        esac
    done
done
