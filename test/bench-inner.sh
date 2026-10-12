#!/usr/bin/env bash
# Runs INSIDE the nested niri started by bench.sh. Starts waybar on the bench
# config, lets it settle, samples its CPU over BENCH_SECS, writes the result and
# exits; bench.sh's outer `timeout cage` tears the nested stack down. Like
# orchestrate.sh, it never quits or kills a compositor.
set -u

# Hard guard: only ever run inside bench.sh's private runtime dir.
case "${XDG_RUNTIME_DIR:-}" in
  /tmp/claude-1000/pwbench.*) ;;
  *) echo "REFUSING: XDG_RUNTIME_DIR='${XDG_RUNTIME_DIR:-}' is not bench.sh's" >&2; exit 1 ;;
esac
# The nested niri's socket may well be named like the live one (cage holds
# wayland-0, so niri takes wayland-1) — what matters is that it lives in the
# private runtime dir above, so it can't be the live session's.
if [ -z "${WAYLAND_DISPLAY:-}" ] || [ ! -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]; then
  echo "REFUSING: no nested compositor socket (WAYLAND_DISPLAY='${WAYLAND_DISPLAY:-}')" >&2
  exit 1
fi

out="$BENCH_OUT/bench-result.txt"
rm -f "$out"
waybar -c "$BENCH_CFG" -s "$BENCH_CSS" >"$BENCH_OUT/bench-waybar.log" 2>&1 &
wb=$!

sleep 10 # llvmpipe is slow: let the bars map and every tile get its data

ticks() { awk '{print $14 + $15}' "/proc/$wb/stat"; }
hz=$(getconf CLK_TCK)
t0=$(ticks)
s0=$(date +%s.%N)
sleep "$BENCH_SECS"
t1=$(ticks)
s1=$(date +%s.%N)
grim "$BENCH_OUT/bench-shot.png" 2>/dev/null

{
  echo "waybar pid $wb, ${BENCH_SECS}s sample"
  awk -v a="$t0" -v b="$t1" -v hz="$hz" -v s0="$s0" -v s1="$s1" \
    'BEGIN { printf "cpu: %.1f%% of one core\n", 100 * (b - a) / hz / (s1 - s0) }'
  grep -E '^(VmRSS|Threads)' "/proc/$wb/status"
} >"$out"
kill "$wb" 2>/dev/null
