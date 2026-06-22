#!/usr/bin/env bash
# Screenshot of the tile in a REAL (nested) waybar — exercises the actual CFFI
# path (wbcffi_init, GtkContainer handoff, GtkGLArea, femtovg). This is the
# faithful integration test the offscreen render can't provide.
#
# History: this logged the user out 3x. Root cause was a `pkill -x niri`
# teardown in orchestrate.sh that matched the LIVE session's process (same comm
# name). That line is gone; teardown is now SOLELY `timeout` killing the cage
# PID below (see the safety knobs and the orchestrate.sh guard). The earlier
# seat/GPU theories were wrong, but the noop-seat + software-render knobs remain
# as defense in depth.
#
# Two isolations, removing the two ways this can log you out:
#  1. SEAT: LIBSEAT_BACKEND=noop forces libseat to a stub that never contacts
#     logind/seatd — so it CANNOT acquire/steal the active seat.
#  2. GPU: WLR_RENDERER=pixman + LIBGL_ALWAYS_SOFTWARE=1 + GALLIUM_DRIVER=llvmpipe
#     force the ENTIRE stack (cage, niri, waybar's GtkGLArea/femtovg) to render
#     in software. llvmpipe/pixman never open /dev/dri, so there is no GPU to
#     reset on teardown (a hardware-GL nested stack reset the shared GPU and
#     killed the live session — see test notes).
#
#   cage(headless, pixman) -> niri(winit, llvmpipe) -> waybar(cffi/pwetty) -> grim
#
# Output: $OUT (default /tmp/claude-1000/pwetty-shot.png) + a *-crop.png.
set -u
here="$(cd "$(dirname "$0")" && pwd)"

WLR_BACKENDS=headless \
WLR_RENDERER=pixman \
LIBSEAT_BACKEND=noop \
WLR_LIBINPUT_NO_DEVICES=1 \
LIBGL_ALWAYS_SOFTWARE=1 \
GALLIUM_DRIVER=llvmpipe \
  timeout -k 5 22 cage -- niri -c "$here/niri.kdl" -- bash "$here/orchestrate.sh"

rc=$?
# Watchdog: ensure nothing we spawned lingers.
pkill -u "$USER" -x cage 2>/dev/null
echo "shot.sh done (rc=$rc)"
