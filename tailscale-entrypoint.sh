#!/bin/sh
set -eu
PATH="/usr/local/bin:$PATH"

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2> /dev/null && wait "$APP_PID" || true
    tailscale --socket="$TS_SOCKET" down 2> /dev/null || true
    [ -n "${TAILSCALED_PID:-}" ] && kill "$TAILSCALED_PID" 2> /dev/null && wait "$TAILSCALED_PID" || true
}
trap cleanup INT TERM EXIT

tailscaled --socket="$TS_SOCKET" &> /dev/null &
TAILSCALED_PID=$!

if [ -n "${TS_HOSTNAME:-}" ]; then
    tailscale --socket="$TS_SOCKET" set --hostname="$TS_HOSTNAME"
fi

tailscale --socket="$TS_SOCKET" up --authkey="${TS_AUTHKEY:-}" \
    --advertise-routes="$TS_ROUTES" --snat-subnet-routes=false ${TS_EXTRA_ARGS:-}
tailscale --socket="$TS_SOCKET" status --peers=false

/sidestore-vpn "$@" &
APP_PID=$!
wait "$APP_PID"
