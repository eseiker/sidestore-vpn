#!/bin/sh
set -e

if [ -z "${TS_AUTHKEY:-}" ]; then
    echo "TS_AUTHKEY is required to bring up Tailscale" >&2
    exit 1
fi

/usr/local/bin/tailscaled &
TAILSCALED_PID=$!

cleanup() {
    kill "${TAILSCALED_PID}" 2>/dev/null || true
}
trap cleanup INT TERM EXIT

/usr/local/bin/tailscale up

# Run sidestore-vpn in the foreground and ensure tailscaled is cleaned up
set +e
/sidestore-vpn "$@"
EXIT_CODE=$?
set -e
cleanup
exit "$EXIT_CODE"
