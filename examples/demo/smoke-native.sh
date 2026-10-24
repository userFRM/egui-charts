#!/usr/bin/env bash
# Verify the native window opens and survives its first rendered frames under X11.
# Usage: xvfb-run -a examples/demo/smoke-native.sh <demo-binary>
set -euo pipefail
"$1" &
demo_pid=$!
trap 'kill "$demo_pid" 2>/dev/null || true; wait "$demo_pid" 2>/dev/null || true' EXIT
for _ in {1..60}; do
    kill -0 "$demo_pid" 2>/dev/null || { echo "native demo exited before startup" >&2; exit 1; }
    if xdotool search --onlyvisible --pid "$demo_pid" --name 'egui-charts demo' >/dev/null 2>&1; then
        sleep 3
        kill -0 "$demo_pid"
        echo 'native demo: window opened and process survived rendering'
        exit 0
    fi
    sleep 0.5
done
echo 'native demo did not open a window within 30 seconds' >&2
exit 1
