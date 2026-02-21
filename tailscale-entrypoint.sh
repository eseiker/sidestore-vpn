#!/bin/sh
set -eu
PATH="/usr/local/bin:$PATH"

APP_PID=""
TAILSCALED_PID=""

cleanup() {
    [ -n "$APP_PID" ] && kill -TERM "$APP_PID" >/dev/null 2>&1 || true
    tailscale --socket="$TS_SOCKET" down >/dev/null 2>&1 || true
    [ -n "$TAILSCALED_PID" ] && kill -TERM "$TAILSCALED_PID" >/dev/null 2>&1 || true
    [ -n "$TAILSCALED_PID" ] && wait "$TAILSCALED_PID" >/dev/null 2>&1 || true
    tailscaled --cleanup >/dev/null 2>&1 || true
}

trap cleanup EXIT
trap 'trap - EXIT; cleanup; exit 0' INT TERM

mkdir -p "$(dirname "$TS_SOCKET")" "$TS_STATE_DIR"
tailscaled --socket="$TS_SOCKET" --statedir="$TS_STATE_DIR" >/dev/null 2>&1 &
TAILSCALED_PID=$!

# Wait for tailscaled socket to become available
timeout 30 sh -c 'until [ -S "$TS_SOCKET" ]; do sleep 0.1; done'

tailscale --socket="$TS_SOCKET" up --authkey="${TS_AUTHKEY:-}" \
    --advertise-routes="$TS_ROUTES" --snat-subnet-routes=false \
    ${TS_HOSTNAME:+--hostname="$TS_HOSTNAME"} ${TS_EXTRA_ARGS:-}
tailscale --socket="$TS_SOCKET" status --peers=false

/sidestore-vpn "$@" &
APP_PID=$!
set +e
wait "$APP_PID"
APP_STATUS=$?
set -e
exit "$APP_STATUS"
