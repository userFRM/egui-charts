#!/usr/bin/env bash
# Verify the native window opens and survives its first rendered frames under X11.
# Usage: xvfb-run -a -s '-screen 0 1280x1024x24 -noreset' examples/demo/smoke-native.sh <demo-binary>
set -euo pipefail
# xvfb-run can hand over before the server accepts clients; start the demo once the display answers.
display_ready=false
for _ in {1..20}; do
    if xdotool getdisplaygeometry >/dev/null 2>&1; then
        display_ready=true
        break
    fi
    sleep 0.5
done
if [[ $display_ready != true ]]; then
    echo "native demo: X11 display ${DISPLAY:-<unset>} did not accept clients within 10 seconds" >&2
    exit 1
fi
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
