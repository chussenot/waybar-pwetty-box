#!/usr/bin/env bash
# CPU benchmark of a REAL waybar hosting a bar full of claude tiles, in a nested
# headless stack — the live-shaped load (~40 tiles: 7 bars x 6, so every tile
# fits the 1280x720 headless output) that examples/bench_frames only
# approximates offscreen.
#
#   test/bench.sh [seconds] [tiles-per-bar] [bars]       (default 30 6 7)
#
# Same isolation as shot.sh (read its header): headless cage on pixman, a noop
# seat, software GL everywhere — plus its OWN XDG_RUNTIME_DIR, so the nested
# compositor's sockets can never be mistaken for the live session's. Teardown is
# `timeout` killing the cage PID it launched; nothing is targeted by name.
#
#   cage(headless, pixman) -> niri(winit, llvmpipe) -> bench-inner.sh -> waybar
#
# Tiles stream a fixed sample (tiles/claude/samples), cycling through them, so
# blinking/working/prompt/idle states animate exactly as on the live bar. Stream
# mode reads one JSON object per line, hence `jq -c` on the pretty samples.
# Output: CPU%/RSS/threads of the nested waybar, written to $OUT (and stdout).
set -u
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
secs="${1:-30}"
per_bar="${2:-6}"
bars="${3:-7}"
scratch="${SCRATCH:-/tmp/claude-1000/pwetty-bench}"
mkdir -p "$scratch"
so="${SO:-$root/target/release/libpwetty_box.so}"
[ -f "$so" ] || { echo "no module at $so (cargo build --release)" >&2; exit 1; }

# One waybar config: `bars` bars, each with `per_bar` claude tiles.
cfg="$scratch/bench-waybar.jsonc"
# BENCH_SAMPLES=window,idle limits the cycle to those samples (by file stem).
samples=()
for f in "$root"/tiles/claude/samples/*.json; do
  stem="$(basename "$f" .json)"
  case ",${BENCH_SAMPLES:-$stem}," in *",$stem,"*) samples+=("$f") ;; esac
done
# BENCH_FPS=N pins every claude tile's animation rate (default: the module's).
jq -n --arg so "$so" --argjson bars "$bars" --argjson per "$per_bar" \
  --argjson fps "${BENCH_FPS:-null}" \
  --args '$ARGS.positional as $s
  | [range($bars) as $b
     | { layer: "top", position: "top", height: 100,
         "modules-left": [range($per) | "cffi/pwetty#\($b)-\(.)"] }
       + ([range($per) as $i
           | { key: "cffi/pwetty#\($b)-\($i)",
               value: { module_path: $so, tile: "claude", stream: true, width: 179,
                        exec: "jq -c . \($s[($b * $per + $i) % ($s | length)]); sleep infinity" }
                      + (if $fps then { fps: $fps } else {} end) }]
          | from_entries)]' "${samples[@]}" >"$cfg" || exit 1

# BENCH_MIX=gl: one bar of every GL-backed tile kind instead — the femtovg demo
# tile, a background shader, span <glow> and a <bg> preset, two of each — so
# they all share (and must not corrupt) the one offscreen GL context.
if [ "${BENCH_MIX:-}" = gl ]; then
  jq -n --arg so "$so" --arg sh "$root/examples/shaders/aurora.glsl" '
    def m($f): { module_path: $so, width: 150, height: 56 } + $f;
    [ { layer: "top", position: "top", height: 64,
        "modules-left": [range(8) | "cffi/pwetty#gl-\(.)"] }
      + ([ m({}), m({}),
           m({ background_shader: $sh, text: "shader", fps: 30 }),
           m({ background_shader: $sh, text: "shader", fps: 30 }),
           m({ text: "x", format: "<glow color=\"#f38ba8\"><b>glow</b></glow>", fps: 30 }),
           m({ text: "x", format: "<glow color=\"#a6e3a1\"><b>glow</b></glow>", fps: 30 }),
           m({ text: "x", format: "<bg preset=\"night\"/><sep/>bg night", fps: 30 }),
           m({ text: "x", format: "<bg preset=\"caustic\"/><sep/>bg caustic", fps: 30 }) ]
         | to_entries | map({ key: "cffi/pwetty#gl-\(.key)", value }) | from_entries) ]' \
    >"$cfg" || exit 1
fi

# BENCH_SCALE=1.25 runs the nested output at that scale — GTK then draws at
# its integer ceiling (2), the HiDPI path a 1x run never exercises.
niri_cfg="$here/niri.kdl"
if [ -n "${BENCH_SCALE:-}" ]; then
  niri_cfg="$scratch/bench-niri.kdl"
  sed "s|^    mode \"1280x720\"$|    mode \"1280x720\"\n    scale ${BENCH_SCALE}|" "$here/niri.kdl" >"$niri_cfg"
  grep -q "scale ${BENCH_SCALE}" "$niri_cfg" || { echo "BENCH_SCALE: couldn't patch niri.kdl" >&2; exit 1; }
fi

# A private runtime dir, short enough for a Unix socket path (108 bytes).
runtime="$(mktemp -d /tmp/claude-1000/pwbench.XXXXXX)" || exit 1
trap 'rm -rf "$runtime"' EXIT

XDG_RUNTIME_DIR="$runtime" \
WAYLAND_DISPLAY= \
DISPLAY= \
NIRI_SOCKET= \
WLR_BACKENDS=headless \
WLR_RENDERER=pixman \
LIBSEAT_BACKEND=noop \
WLR_LIBINPUT_NO_DEVICES=1 \
LIBGL_ALWAYS_SOFTWARE=1 \
GALLIUM_DRIVER=llvmpipe \
BENCH_CFG="$cfg" BENCH_CSS="$here/waybar/style.css" BENCH_SECS="$secs" \
BENCH_OUT="$scratch" \
  timeout -k 5 $((secs + 30)) cage -- niri -c "$niri_cfg" -- bash "$here/bench-inner.sh"
rc=$?
echo "bench.sh done (rc=$rc)"
cat "$scratch/bench-result.txt" 2>/dev/null || echo "no result (see $scratch/bench-waybar.log)"
