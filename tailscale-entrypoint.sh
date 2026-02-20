#!/bin/sh
set -eu
PATH="/usr/local/bin:$PATH"

APP_PID=""
TAILSCALED_PID=""

cleanup() {
    [ -n "$APP_PID" ] && kill -TERM "$APP_PID" &> /dev/null || true
    [ -n "$APP_PID" ] && wait "$APP_PID" &> /dev/null || true
    tailscale --socket="$TS_SOCKET" down &> /dev/null || true
    [ -n "$TAILSCALED_PID" ] && kill -TERM "$TAILSCALED_PID" &> /dev/null || true
    [ -n "$TAILSCALED_PID" ] && wait "$TAILSCALED_PID" &> /dev/null || true
    tailscaled --cleanup &> /dev/null || true
}

trap cleanup EXIT
trap 'trap - EXIT; cleanup; exit 0' INT TERM

mkdir -p "$(dirname "$TS_SOCKET")" "$TS_STATE_DIR"
tailscaled --socket="$TS_SOCKET" --statedir="$TS_STATE_DIR" &> /dev/null &
TAILSCALED_PID=$!

if [ -n "${TS_HOSTNAME:-}" ]; then
    tailscale --socket="$TS_SOCKET" set --hostname="$TS_HOSTNAME"
fi

tailscale --socket="$TS_SOCKET" up --authkey="${TS_AUTHKEY:-}" \
    --advertise-routes="$TS_ROUTES" --snat-subnet-routes=false ${TS_EXTRA_ARGS:-}
tailscale --socket="$TS_SOCKET" status --peers=false

/sidestore-vpn "$@" &
APP_PID=$!
set +e
wait "$APP_PID"
APP_STATUS=$?
set -e
exit "$APP_STATUS"
