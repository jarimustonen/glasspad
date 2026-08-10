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

# Isolate the loopback-server pid file so this suite's `serve` invocations never
# touch the developer's real ~/.glasspad/server.pid (which a running local deploy
# may own). Each `serve` here writes/reclaims this hermetic path instead; the
# existing EXIT-trap cleanup() removes it (the later traps all chain through it).
export GLASSPAD_PID_FILE="$(mktemp -u "${TMPDIR:-/tmp}/glasspad-sec-pid.XXXXXX")"

# Isolate the return-channel submission store to a hermetic temp dir so this suite's
# `serve` invocations never write to the developer's real ~/.glasspad/submissions.
export GLASSPAD_STATE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/glasspad-sec-state.XXXXXX")"

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
cleanup() { kill "$SERVER_PID" 2>/dev/null || true; rm -f "$GLASSPAD_PID_FILE" 2>/dev/null || true; rm -rf "$GLASSPAD_STATE_DIR" 2>/dev/null || true; }
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
printf '<!doctype html><title>Sales Q3</title><h1>sales</h1>' > "$WORK/myspace/sales.html"
# Wave 4 nav-injection probe (server side): a hostile artifact TITLE that the
# resolver DECODES to raw markup as text. The trusted parent nav must never emit
# it as executable markup — it lives in the nav data literal JSON-for-script
# encoded, and is inserted client-side via textContent.
printf '<!doctype html><title>&quot;&gt;&lt;img src=x onerror=alert(1)&gt;&lt;script&gt;alert(2)&lt;/script&gt;</title><h1>inj</h1>' > "$WORK/myspace/inject.html"
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
# No wildcard CORS on user assets: a foreign web page the user has open must not
# be able to `fetch()` and read a space's assets cross-origin.
[ -z "$(hdr "$SB/myspace/assets/data.json" access-control-allow-origin)" ]; scheck $? "asset has NO Access-Control-Allow-Origin (no cross-origin read)"

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

# Wave 4 nav chrome: the trusted shell lists the space's artifacts, and an
# artifact-derived title can NEVER become live markup in the parent.
SHELL_HTML="$(curl -s "$SB/myspace/inject")"
echo "$SHELL_HTML" | grep -q 'id="gp-nav"'; scheck $? "shell renders the nav chrome container"
echo "$SHELL_HTML" | grep -q '"slug":"sales"'; scheck $? "nav table lists the space's sibling artifacts"
# The hostile title must NOT appear as raw executable markup anywhere in the shell.
! echo "$SHELL_HTML" | grep -qi '<img src=x onerror'; scheck $? "hostile title is not emitted as raw <img onerror> markup"
! echo "$SHELL_HTML" | grep -qi '<script>alert(2)'; scheck $? "hostile title is not emitted as a raw <script> element"
# It IS present, but JSON-for-script encoded (<…) in the nav data literal.
echo "$SHELL_HTML" | grep -q '\\u003cimg src=x onerror'; scheck $? "hostile title survives only as \\u003c-encoded text in the nav data"
# The trusted shell enforces Trusted Types (any accidental innerHTML sink throws).
echo "$(hdr "$SB/myspace/inject" content-security-policy)" | grep -q "require-trusted-types-for 'script'"; scheck $? "shell CSP enforces Trusted Types"

# Egress boundary held: the artifact CSP keeps `connect-src 'none'` — live reload
# is shell-side (its own connect-src 'self'), so the artifact stays fully closed.
CSP="$(hdr "$SB/myspace/_c/index" content-security-policy)"
echo "$CSP" | grep -q "connect-src 'none';"; scheck $? "artifact connect-src stays 'none' (fully closed)"
! echo "$CSP" | grep -q "/_gp/reload"; scheck $? "SSE path is not named in the artifact CSP"

# ---------------------------------------------------------------------------
# Return channel (the airlock). The trusted shell POSTs a submission; the ARTIFACT
# never gains egress. These are server-side HTTP checks (a browser can't help with
# CSRF/rate/spoof at the network layer); the browser drives the full artifact→shell
# →server chain in run.mjs above.
# ---------------------------------------------------------------------------
LB_ORIGIN="http://127.0.0.1:$SPACE_PORT"
sub_post() { # origin  body  -> http_code
  curl -s -o /dev/null -w '%{http_code}' -X POST "$SB/myspace/_gp/submit" \
    -H "Origin: $1" -H 'content-type: application/json' -d "$2"
}

# AIRLOCK REGRESSION (MUST hold): the artifact CSP is still `connect-src 'none'`
# AND the sandbox grants NO `allow-forms` — the return channel opened no egress.
! echo "$CSP" | grep -q "allow-forms"; scheck $? "airlock: artifact sandbox still has NO allow-forms"
echo "$CSP" | grep -q "sandbox allow-scripts allow-top-navigation-by-user-activation"; scheck $? "airlock: artifact sandbox tokens unchanged (no new grant)"

# Same-origin submit is accepted; a cross-origin (CSRF) submit is rejected.
[ "$(sub_post "$LB_ORIGIN" '{"data":{"a":1},"slug":"index"}')" = "201" ]; scheck $? "return: same-origin submit accepted (201)"
[ "$(sub_post "http://evil.example" '{"data":{}}')" = "403" ]; scheck $? "return: cross-origin submit rejected (CSRF 403)"

# Content-version / cross-round mismatch is rejected (409).
[ "$(sub_post "$LB_ORIGIN" '{"data":{},"content_version":"00000000deadbeef","slug":"index"}')" = "409" ]; scheck $? "return: stale content-version rejected (409)"

# Cross-space spoof: a submission is bound to the URL-path space (myspace), never a
# payload field. A submit to myspace lands under myspace; an unrelated space is empty.
sub_post "$LB_ORIGIN" '{"data":{"tag":"bound"},"slug":"sales"}' >/dev/null
curl -s "$SB/myspace/_gp/submissions" | grep -q '"key":"myspace"'; scheck $? "return: submission keyed by the URL-path space, not a payload field"
curl -s "$SB/otherspace/_gp/submissions" | grep -q '"submissions":\[\]'; scheck $? "return: an unrelated space has no submissions (no cross-space leak)"

# Flood / rate limit: a burst of submits eventually 429s (per-space rate cap).
RL=0
for i in $(seq 1 40); do
  [ "$(sub_post "$LB_ORIGIN" '{"data":{"i":'"$i"'}}')" = "429" ] && RL=1 && break
done
[ "$RL" = "1" ]; scheck $? "return: a submit flood is rate-limited (429)"

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
