#!/usr/bin/env bash
#
# Runs a command inside a headless sway session, and cleans up after it.
#
#     ci/headless-session.sh target/release/clickwork --selftest session
#
# Why this exists: every test in the suite passes with no compositor at all, so
# nothing in it can notice a Wayland protocol binding wrongly. This is the other
# half - a real wlroots compositor, with no screen, no graphics card and no human,
# that a test can be pointed at. It is the same thing in CI and on a development
# machine, which is the point: a failure here is reproducible by running the same
# line locally.
#
# It needs no /dev/dri. `WLR_BACKENDS=headless` skips DRM entirely and
# `WLR_RENDERER=pixman` renders on the CPU, so this runs on a hosted runner that
# has no graphics device of any kind. That is also why sway is the compositor
# here rather than Hyprland: Hyprland requires a DRM node and fails at
# `CBackend::create()` without one, so it cannot run in CI at all. To prove the
# DRM node is genuinely unused, run this under
# `bwrap --dev-bind / / --tmpfs /dev/dri` on a machine that has one.
#
# Sway 1.9 (wlroots 0.17), which is what Ubuntu 24.04 ships, offers every
# protocol this program uses today. It does not offer the `ext_*` successors -
# those need a newer sway than any runner image carries, so nothing here can
# exercise them yet.

set -euo pipefail

if [ $# -eq 0 ]; then
    echo "usage: $0 <command> [args...]" >&2
    exit 2
fi

if ! command -v sway >/dev/null 2>&1; then
    echo "sway is not installed, and this script is nothing without it." >&2
    echo "  Arch:   sudo pacman -S sway" >&2
    echo "  Debian: sudo apt-get install sway" >&2
    exit 127
fi

# A runtime directory of our own, so the session cannot collide with the one the
# person running this is already sitting in, and so its socket is easy to find.
# Wayland refuses a runtime directory anyone else can read.
runtime="$(mktemp -d)"
chmod 700 "$runtime"

config="$runtime/sway.conf"
cat >"$config" <<'CONF'
# Deliberately not the distribution's config: that one binds a terminal that is
# not installed here and starts a bar, and sway complains about both on a runner.
# A fixed output size keeps the geometry checks reading the same numbers every run.
output HEADLESS-1 resolution 1920x1080 position 0 0
default_border none
CONF

sway_pid=""
cleanup() {
    local code=$?
    if [ -n "$sway_pid" ] && kill -0 "$sway_pid" 2>/dev/null; then
        kill "$sway_pid" 2>/dev/null || true
        # Give it a moment to go quietly before insisting.
        for _ in $(seq 1 20); do
            kill -0 "$sway_pid" 2>/dev/null || break
            sleep 0.1
        done
        kill -9 "$sway_pid" 2>/dev/null || true
        wait "$sway_pid" 2>/dev/null || true
    fi
    rm -rf "$runtime"
    exit $code
}
trap cleanup EXIT INT TERM

# Sway inherits its parent's session otherwise and tries to nest inside it, which
# is not what is being tested and does not exist on a runner anyway.
unset WAYLAND_DISPLAY DISPLAY

echo "== starting headless sway (runtime $runtime)"
env XDG_RUNTIME_DIR="$runtime" \
    WLR_BACKENDS=headless \
    WLR_RENDERER=pixman \
    WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$config" >"$runtime/sway.log" 2>&1 &
sway_pid=$!

# Sway picks the first free socket name in the runtime directory rather than
# taking one it is given, so it is found rather than assumed.
display=""
for _ in $(seq 1 100); do
    if ! kill -0 "$sway_pid" 2>/dev/null; then
        echo "sway exited before it opened a socket:" >&2
        cat "$runtime/sway.log" >&2
        exit 1
    fi
    for sock in "$runtime"/wayland-*; do
        case "$sock" in
        *.lock | *'wayland-*') continue ;;
        esac
        [ -S "$sock" ] || continue
        display="$(basename "$sock")"
        break 2
    done
    sleep 0.1
done

if [ -z "$display" ]; then
    echo "sway opened no socket within ten seconds:" >&2
    cat "$runtime/sway.log" >&2
    exit 1
fi

echo "== sway is up on $display"
echo "== running: $*"
echo

set +e
env XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$display" "$@"
code=$?
set -e

echo
echo "== command exited $code"
if [ $code -ne 0 ] && [ -s "$runtime/sway.log" ]; then
    echo "== what sway had to say"
    cat "$runtime/sway.log"
fi
exit $code
