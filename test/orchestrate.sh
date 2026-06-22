#!/usr/bin/env bash
# Runs INSIDE nested niri (WAYLAND_DISPLAY -> the nested niri's socket).
# Starts waybar with our CFFI module, waits for the GL widget to realize and
# paint, captures a frame, then EXITS. It never quits/kills any compositor —
# shot.sh's outer `timeout cage` tears the nested stack down by killing the
# specific cage PID it launched. No command here targets a compositor by name
# or by the live session's socket.
set -u

# Hard guard: refuse to run against the user's live compositor. The nested niri
# hands us its own WAYLAND_DISPLAY; the live session is wayland-1. If we somehow
# aren't nested, abort before waybar/grim can touch the real desktop.
if [ "${WAYLAND_DISPLAY:-}" = "wayland-1" ] || [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "REFUSING: not in a nested compositor (WAYLAND_DISPLAY='${WAYLAND_DISPLAY:-}')" >&2
  exit 1
fi
echo "nested WAYLAND_DISPLAY=$WAYLAND_DISPLAY  NIRI_SOCKET=${NIRI_SOCKET:-unset}"

here="$(cd "$(dirname "$0")" && pwd)"
out="${OUT:-/tmp/claude-1000/pwetty-shot.png}"
logdir="$(dirname "$out")"

waybar -c "$here/waybar/config.jsonc" -s "$here/waybar/style.css" \
  >"$logdir/pwetty-waybar.log" 2>&1 &

sleep 8   # software (llvmpipe) is slow: let the bar map, GLArea realize, paint

grim "$out" 2>"$logdir/pwetty-grim.log"
echo "grim(full) rc=$? -> $out"

# Close-up crop of the top strip where the bar lives.
grim -g "0,0 1280x110" "${out%.png}-crop.png" 2>>"$logdir/pwetty-grim.log"
echo "grim(crop) rc=$? -> ${out%.png}-crop.png"

# DONE. We do NOT quit or kill anything here — not even the nested niri. The
# `niri msg action quit || pkill -x niri` that used to live here is what logged
# the user out 3x (the live session's process is also named `niri`). shot.sh's
# outer `timeout cage` kills the specific cage PID, which tears down the nested
# niri+waybar with it. No compositor is ever targeted by name from this script.
echo "capture done; exiting — outer 'timeout cage' will tear down the nested stack"
