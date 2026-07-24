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

# ---------------------------------------------------------------------------
# Wave 2a — space-model server-side probes. Path traversal and symlink escape
# are enforced by the SERVER (a browser can't help), so these are HTTP/exit-code
# checks against a live directory rather than browser assertions. They extend the
# adversarial suite with the new attack surface (asset routing + directory scan).
# ---------------------------------------------------------------------------
echo "==> Running Wave 2a space-model probes (traversal / symlink / hostile asset)"
SPACE_FAILURES=0
scheck() { # name  condition(0=pass)
  if [ "$1" = "0" ]; then echo "PASS  $2"; else echo "FAIL  $2"; SPACE_FAILURES=$((SPACE_FAILURES+1)); fi
}

WORK="$(mktemp -d)"
SPACE_PORT=$((PORT+1))
cleanup_space() { kill "${SPACE_PID:-0}" 2>/dev/null || true; rm -rf "$WORK"; }
trap 'cleanup; cleanup_space' EXIT

# A clean, servable space. `secret` lives OUTSIDE it — the symlink probe targets it.
printf 'SECRET-OUTSIDE-THE-SPACE' > "$WORK/secret.txt"
mkdir -p "$WORK/myspace/assets/sub"
printf '<!doctype html><title>Home</title><h1>hi</h1>' > "$WORK/myspace/index.html"
printf 'console.log(1)' > "$WORK/myspace/assets/app.js"
# A hostile SVG asset: must be neutralized (served with `Content-Security-Policy: sandbox`).
printf '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>' > "$WORK/myspace/assets/logo.svg"
printf '{"x":1}' > "$WORK/myspace/assets/sub/data.json"

pkill -f "target/debug/glasspad serve" 2>/dev/null || true
sleep 0.5
./target/debug/glasspad serve --port "$SPACE_PORT" "$WORK/myspace" >/tmp/glasspad-space-test.log 2>&1 &
SPACE_PID=$!
for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$SPACE_PORT/myspace/_c/index" >/dev/null 2>&1; then break; fi
  sleep 0.25
done
SB="http://127.0.0.1:$SPACE_PORT"

code() { curl -s -o /dev/null -w "%{http_code}" "$1"; }
hdr()  { curl -s -D- -o /dev/null "$1" | tr -d '\r' | awk -F': ' "tolower(\$1)==\"$2\"{print \$2}"; }

# Real content + assets serve.
[ "$(code "$SB/myspace/_c/index")" = "200" ]; scheck $? "live artifact served (200)"
[ "$(code "$SB/myspace/assets/app.js")" = "200" ]; scheck $? "real asset served (200)"
[ "$(code "$SB/myspace/assets/sub/data.json")" = "200" ]; scheck $? "nested asset served (200)"

# Asset hardening: nosniff + a `sandbox` CSP so a hostile top-level SVG/HTML asset
# runs script-less in a null origin.
[ "$(hdr "$SB/myspace/assets/logo.svg" x-content-type-options)" = "nosniff" ]; scheck $? "asset carries nosniff"
[ "$(hdr "$SB/myspace/assets/logo.svg" content-security-policy)" = "sandbox" ]; scheck $? "hostile SVG asset is sandboxed (top-level neutralized)"
[ "$(hdr "$SB/myspace/assets/app.js" content-type)" = "text/javascript; charset=utf-8" ]; scheck $? "asset MIME detected"

# Path-traversal probes — every one must 404 (never resolve outside the space).
for p in \
  "assets/../../secret.txt" \
  "assets/..%2f..%2fsecret.txt" \
  "assets/%2e%2e/%2e%2e/secret.txt" \
  "assets/....//secret.txt" \
  "assets/sub/../../../secret.txt" ; do
  c="$(code "$SB/myspace/$p")"
  [ "$c" = "404" ] || [ "$c" = "400" ]; scheck $? "traversal blocked: $p (got $c)"
done
# The secret is never reachable by any name.
! curl -s "$SB/myspace/assets/../../secret.txt" | grep -q SECRET-OUTSIDE; scheck $? "secret file never served via traversal"

# Egress boundary held: the artifact CSP names ONLY the loopback SSE path, never
# a bare origin (which would re-open /api/*) and never a foreign host.
CSP="$(hdr "$SB/myspace/_c/index" content-security-policy)"
echo "$CSP" | grep -q "connect-src http://127.0.0.1:$SPACE_PORT/_gp/reload http://localhost:$SPACE_PORT/_gp/reload;"; scheck $? "artifact connect-src scoped to SSE reload path only"
! echo "$CSP" | grep -qE "connect-src [^;]*(\*|:$SPACE_PORT;| :$SPACE_PORT )"; scheck $? "artifact connect-src is not a wildcard/bare origin"

kill "$SPACE_PID" 2>/dev/null || true
sleep 0.3

# Symlink escape: a space containing a symlinked artifact is a hard scan error —
# `serve` must refuse to start (non-zero) and name the symlink, so a crafted link
# can never expose a file outside the space.
mkdir -p "$WORK/linkspace"
ln -s "$WORK/secret.txt" "$WORK/linkspace/index.html"
if ./target/debug/glasspad serve --port "$SPACE_PORT" "$WORK/linkspace" >/tmp/glasspad-link-test.log 2>&1; then
  scheck 1 "symlinked artifact refused at startup"
else
  grep -qi "symlink" /tmp/glasspad-link-test.log; scheck $? "symlinked artifact refused with informative error"
fi

# Reserved-name / collision: hard errors, refuse to start.
mkdir -p "$WORK/reserved"; printf x > "$WORK/reserved/api.html"; printf x > "$WORK/reserved/index.html"
if ./target/debug/glasspad serve --port "$SPACE_PORT" "$WORK/reserved" >/tmp/glasspad-reserved-test.log 2>&1; then
  scheck 1 "reserved slug refused at startup"
else
  grep -qi "reserved" /tmp/glasspad-reserved-test.log; scheck $? "reserved slug refused with informative error"
fi

echo ""
if [ "$SPACE_FAILURES" -eq 0 ]; then
  echo "✅ Wave 2a space-model probes PASSED"
else
  echo "❌ $SPACE_FAILURES Wave 2a space-model probe(s) FAILED"
  exit 1
fi
