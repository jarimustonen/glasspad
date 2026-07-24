// Wave 1 adversarial browser suite — the security gate for the glasspad v0.2
// HTML-artifact host. Drives a real (headless Chromium) browser against a
// running glasspad server and asserts the sandbox + CSP contract actually holds
// as the *browser* enforces it — not just that our server emits the headers.
//
// Env:
//   GLASSPAD_PORT   port the glasspad server is already listening on (required)
//   HEADED=1        run headed (debugging)
//
// Exit code 0 = all assertions passed, 1 = any failure. See design.md §2–§6.

import http from "node:http";
import { chromium } from "playwright";

const GP_PORT = process.env.GLASSPAD_PORT;
if (!GP_PORT) {
  console.error("GLASSPAD_PORT not set");
  process.exit(2);
}
const BASE = `http://127.0.0.1:${GP_PORT}`;

// --- tiny assertion harness ------------------------------------------------
let failures = 0;
const results = [];
function check(name, cond, detail = "") {
  const ok = !!cond;
  if (!ok) failures++;
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  — " + detail : ""}`);
}

// --- network canary: any request the artifact manages to send lands here.
// If CSP works, this counter stays at 0 for every exfil channel.
let canaryHits = [];
const canary = http.createServer((req, res) => {
  canaryHits.push(`${req.method} ${req.url}`);
  res.writeHead(204, { "access-control-allow-origin": "*" });
  res.end();
});
canary.on("upgrade", (req, socket) => {
  // A WebSocket handshake that reaches us is also exfil.
  canaryHits.push(`WS ${req.url}`);
  socket.destroy();
});
await new Promise((r) => canary.listen(0, "127.0.0.1", r));
const CANARY_PORT = canary.address().port;

async function main() {
  const browser = await chromium.launch({ headless: !process.env.HEADED });
  const context = await browser.newContext();
  const page = await context.newPage();

  // Capture uncaught page errors (the `./test-browser.sh errors`-clean DoD).
  // CSP *violations* are expected and reported separately; a thrown JS error in
  // a benign artifact is not. Track them and assert the benign path is clean.
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));

  // ---------------------------------------------------------------------
  // TEST 1 — per-channel exfil is blocked by CSP (network ground truth).
  // ---------------------------------------------------------------------
  {
    canaryHits = [];
    const cspSeen = { value: "" };
    page.on("response", (resp) => {
      if (resp.url().includes("/_c/exfil")) {
        cspSeen.value = resp.headers()["content-security-policy"] || "";
      }
    });
    // Direct-open of the raw content route with the canary target in the hash.
    await page.goto(`${BASE}/demo/_c/exfil#canary=${CANARY_PORT}`, {
      waitUntil: "load",
    });
    // Let the probe's async report + any (blocked) requests settle.
    await page.waitForTimeout(700);
    const result = await page.evaluate(() => window.__gpResult);

    check(
      "exfil: response carries `sandbox allow-scripts` CSP (direct-open sandboxed)",
      cspSeen.value.startsWith("sandbox allow-scripts"),
      cspSeen.value.slice(0, 60),
    );
    check(
      "exfil: every FETCH-FAMILY egress channel blocked — canary received nothing",
      canaryHits.length === 0,
      canaryHits.join(", ") || "0 hits",
    );
    check(
      "exfil: browser reported CSP violations for the blocked channels",
      result && Array.isArray(result.violations) && result.violations.length > 0,
      result ? JSON.stringify(result.violations).slice(0, 120) : "no result",
    );
    check(
      "exfil: connect-src 'none' blocks even a request back to the SELF host (not just foreign)",
      result && Array.isArray(result.violations) &&
        result.violations.some((v) => v.includes("connect-src") && v.includes("/api/")),
      result ? JSON.stringify(result.violations).slice(0, 200) : "no result",
    );
    check(
      "exfil: the ALLOWED channel still works — self-host <script> loaded",
      result && result.selfScriptLoaded === true,
      result ? `selfScriptLoaded=${result.selfScriptLoaded}` : "no result",
    );
  }

  // ---------------------------------------------------------------------
  // TEST 1b — self-navigation channel. The reviewers flagged iframe
  // self-navigation as an unblocked exfil channel. It turns out the TRUSTED
  // SHELL closes it: the parent's `frame-src 'self'` governs the framed
  // artifact's own navigations, so a FRAMED artifact cannot navigate its iframe
  // to a foreign origin. We assert that containment here (canary stays empty).
  // (A DIRECT-OPENED, top-level artifact has no parent frame-src and CAN
  // self-navigate — but it also holds no cross-frame secret; that top-level case
  // is the residual the design accepts, mitigated by Wave 4 restoring the doc.)
  // ---------------------------------------------------------------------
  {
    canaryHits = [];
    await page.goto(`${BASE}/demo/`, { waitUntil: "load" });
    await page.evaluate((url) => {
      const f = document.createElement("iframe");
      f.setAttribute("sandbox", "allow-scripts allow-top-navigation-by-user-activation");
      f.src = url;
      document.body.appendChild(f);
    }, `${BASE}/demo/_c/nav-exfil#canary=${CANARY_PORT}`);
    await page.waitForTimeout(600);
    check(
      "framed self-navigation to a foreign origin is CONTAINED by the shell's frame-src 'self'",
      canaryHits.length === 0,
      canaryHits.join(", ") || "0 hits (contained)",
    );
  }

  // ---------------------------------------------------------------------
  // TEST 2 — sandbox escape fails: null origin, no parent/secret/storage.
  // Build the exact shell relationship (sandboxed iframe + a parent secret)
  // and record what the artifact manages to reach.
  // ---------------------------------------------------------------------
  {
    // Frame the artifact from a glasspad-ORIGIN page — the artifact's
    // `frame-ancestors http://127.0.0.1:PORT` CSP (correctly) refuses to be
    // framed by any foreign origin, so we reproduce the real shell relationship.
    await page.goto(`${BASE}/demo/`, { waitUntil: "load" });
    const escape = await page.evaluate(async (contentUrl) => {
      return await new Promise((resolve) => {
        window.__shellSecret = "TOP-SECRET-42";
        let received = null;
        window.addEventListener("message", (e) => {
          if (e.data && e.data.type === "gp-test-result" && e.data.result &&
              e.data.result.kind === "escape") received = e.data.result;
        });
        const f = document.createElement("iframe");
        // Exact production sandbox attributes — test what we ship.
        f.setAttribute("sandbox", "allow-scripts allow-top-navigation-by-user-activation");
        f.src = contentUrl;
        document.body.appendChild(f);
        setTimeout(() => resolve(received), 1000);
      });
    }, `${BASE}/demo/_c/escape`);

    check("escape: artifact could NOT read the parent's secret", escape && !escape.stolenSecret,
      escape ? `stolenSecret=${JSON.stringify(escape.stolenSecret)}` : "no report");
    check("escape: reading top-frame location was blocked", escape && escape.topReadBlocked === true);
    check("escape: reading parent document was blocked", escape && escape.parentDocBlocked === true);
    check("escape: cookie access blocked (null origin)", escape && escape.cookieBlocked === true);
    check("escape: localStorage access blocked (null origin)", escape && escape.localStorageBlocked === true);
  }

  // ---------------------------------------------------------------------
  // TEST 3 — direct-open of /{space}/_c/{slug} is sandboxed by the RESPONSE
  // header (opaque origin), independent of any iframe attribute.
  // ---------------------------------------------------------------------
  {
    await page.goto(`${BASE}/demo/_c/index`, { waitUntil: "load" });
    const origin = await page.evaluate(() => {
      try { return String(window.origin); } catch (e) { return "throw"; }
    });
    const cookieBlocked = await page.evaluate(() => {
      try { void document.cookie; return false; } catch (e) { return true; }
    });
    check("direct-open: document has a null (opaque) origin", origin === "null", `origin=${origin}`);
    check("direct-open: cookie access blocked on the opaque origin", cookieBlocked === true);
  }

  // ---------------------------------------------------------------------
  // TEST 4 — postMessage bridge rejects abuse: wrong source, oversized,
  // rapid-fire, malformed schema. Read the parent's bridge stats.
  // ---------------------------------------------------------------------
  {
    await page.goto(`${BASE}/demo/pm-abuse`, { waitUntil: "load" });
    await page.waitForTimeout(400);
    // Wrong-source: the top window messages itself — source !== iframe.
    await page.evaluate(() => window.postMessage({ type: "navigate", slug: "index" }, "*"));
    await page.waitForTimeout(100);
    const stats = await page.evaluate(() => window.__bridgeStats);

    check("bridge: oversized message rejected", stats && stats.rejectedSize >= 1, JSON.stringify(stats));
    check("bridge: malformed-schema message rejected", stats && stats.rejectedSchema >= 1);
    check("bridge: rapid-fire burst rate-limited", stats && stats.rejectedRate >= 1);
    check("bridge: wrong-source message rejected (source !== iframe.contentWindow)",
      stats && stats.rejectedSource >= 1);
    // The fixture floods ~196 well-FORMED navigates in one synchronous burst;
    // the rate cap (20/window) must bound how many are acted on and reject the
    // bulk. Headroom (<=30) absorbs scheduler jitter without hiding a broken cap.
    check("bridge: flood contained — most navigates rate-rejected, accepted stays small",
      stats && stats.accepted <= 30 && stats.rejectedRate >= 100,
      `accepted=${stats && stats.accepted} rejectedRate=${stats && stats.rejectedRate}`);
  }

  // ---------------------------------------------------------------------
  // TEST 5 — Vega-Lite `'unsafe-eval'` question, resolved empirically.
  // With the frozen script-src, `new Function` runs; the `?csp=noeval`
  // diagnostic (strictly tighter) blocks it — proving the dependency.
  // ---------------------------------------------------------------------
  {
    await page.goto(`${BASE}/demo/_c/eval`, { waitUntil: "load" });
    const withEval = await page.evaluate(() => window.__gpResult);
    await page.goto(`${BASE}/demo/_c/eval?csp=noeval`, { waitUntil: "load" });
    const noEval = await page.evaluate(() => window.__gpResult);

    check("vega/eval: `new Function` WORKS under the frozen artifact CSP",
      withEval && withEval.evalWorks === true, withEval ? `evalError=${withEval.evalError}` : "no result");
    check("vega/eval: without 'unsafe-eval' the SAME code is blocked (dependency proven)",
      noEval && noEval.evalWorks === false, noEval ? `evalError=${noEval.evalError}` : "no result");
  }

  // ---------------------------------------------------------------------
  // TEST 6 — the benign home artifact + shell run with no uncaught errors
  // (errors-clean DoD). `?csp=noeval` deliberately triggers an EvalError, so
  // ignore that page; count everything else.
  // ---------------------------------------------------------------------
  {
    // Fresh load of the benign home artifact + shell, in isolation — the
    // exfil/pm-abuse fixtures deliberately provoke blocked-request rejections,
    // so we measure only a known-benign navigation.
    pageErrors.length = 0;
    await page.goto(`${BASE}/demo/`, { waitUntil: "load" });
    await page.waitForTimeout(400);
    check("errors-clean: no uncaught page errors on the benign home path",
      pageErrors.length === 0, pageErrors.join(" | ") || "0 errors");
  }

  await browser.close();
}

try {
  await main();
} catch (err) {
  console.error("SUITE ERROR:", err);
  failures++;
} finally {
  canary.close();
}

console.log(`\n${failures === 0 ? "✅ ALL PASSED" : "❌ " + failures + " FAILED"} (${results.length} checks)`);
process.exit(failures === 0 ? 0 : 1);
