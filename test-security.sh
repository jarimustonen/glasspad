#!/bin/bash
# test-security.sh — Wave 1 adversarial browser suite runner (the security gate).
#
# Builds glasspad, boots it on a loopback test port, drives a real headless
# Chromium (Playwright) through the exfil / sandbox-escape / direct-open /
# postMessage-abuse / Vega-eval probes, and tears everything down.
#
# Usage:  ./test-security.sh            # run the full suite
#         HEADED=1 ./test-security.sh   # watch it in a headed browser
#
# Re-runnable and self-contained: it installs the Playwright browser on first
# run. Requires node (>=18) and cargo. Exit 0 = the security contract holds.
set -euo pipefail
cd "$(dirname "$0")"

PORT="${GLASSPAD_TEST_PORT:-3210}"
SUITE_DIR="tests/security"

echo "==> Building glasspad"
cargo build 2>&1 | tail -1

echo "==> Bootstrapping Playwright (first run downloads Chromium)"
if [ ! -d "$SUITE_DIR/node_modules/playwright" ]; then
  ( cd "$SUITE_DIR" && npm install --silent )
fi
# Ensure the Chromium binary is present (idempotent, cached under ~/.cache).
( cd "$SUITE_DIR" && npx --yes playwright install chromium >/dev/null 2>&1 || true )

echo "==> Starting glasspad on 127.0.0.1:$PORT"
pkill -f "target/debug/glasspad serve" 2>/dev/null || true
sleep 0.5
./target/debug/glasspad serve --port "$PORT" >/tmp/glasspad-sec-test.log 2>&1 &
SERVER_PID=$!
cleanup() { kill "$SERVER_PID" 2>/dev/null || true; }
trap cleanup EXIT

# Wait for the server to answer.
for i in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$PORT/demo/_c/index" >/dev/null 2>&1; then break; fi
  sleep 0.25
done

echo "==> Running adversarial browser suite"
GLASSPAD_PORT="$PORT" node "$SUITE_DIR/run.mjs"
