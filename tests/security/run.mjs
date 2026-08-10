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

  // ---------------------------------------------------------------------
  // TEST 7 — Wave 3b bridge: a same-space RELATIVE link click inside a
  // fragment artifact swaps the iframe via the VALIDATED postMessage bridge
  // (accepted path), while the Wave-1 rejections still hold for the new
  // navigate message (unknown slug rejected; external links not intercepted).
  // Also: the shell's theme toggle re-themes the framed artifact.
  //
  // `nav-a` / `nav-b` are benign FRAGMENT fixtures, so the content route wraps
  // them and injects bridge.js — this drives the real shell↔artifact channel.
  // ---------------------------------------------------------------------
  {
    // Helper: the framed artifact for a given slug (a real content frame). Clicks
    // are dispatched via `element.click()` inside the frame — the bridge listens
    // for the click event regardless, and this sidesteps actionability quirks of
    // driving a null-origin sandboxed frame from the outside.
    const frameFor = (slug) =>
      page.frames().find((f) => new RegExp(`/demo/_c/${slug}$`).test(f.url().replace(/[?#].*$/, "")));
    const waitFrame = async (slug) => {
      let f = null;
      for (let i = 0; i < 80 && !(f = frameFor(slug)); i++) await page.waitForTimeout(50);
      if (f) await f.waitForLoadState?.("load").catch(() => {});
      return f;
    };

    // (b) RELATIVE same-space link swaps the iframe via the VALIDATED bridge.
    await page.goto(`${BASE}/demo/nav-a`, { waitUntil: "load" });
    const navA = await waitFrame("nav-a");
    check("bridge-nav: fragment artifact is framed + bridged (nav-a wrapped)", !!navA,
      navA ? navA.url() : "no artifact frame");
    if (navA) await navA.waitForSelector("#to-b").catch(() => {});

    const parentUrlBefore = page.url();
    const acceptedBefore = await page.evaluate(() => window.__bridgeStats.accepted);
    if (navA) await navA.evaluate(() => document.getElementById("to-b").click());
    await page
      .waitForFunction(() => window.__bridgeStats && window.__bridgeStats.accepted >= 1, { timeout: 4000 })
      .catch(() => {});
    const navB = await waitFrame("nav-b");
    const navBText = navB ? await navB.textContent("h1").catch(() => "") : "";
    const acceptedAfter = await page.evaluate(() => window.__bridgeStats.accepted);
    check("bridge-nav: relative-link click swapped the iframe to nav-b (navigate ACCEPTED + validated)",
      /Nav B/.test(navBText) && acceptedAfter === acceptedBefore + 1,
      `text=${JSON.stringify(navBText)} accepted ${acceptedBefore}->${acceptedAfter}`);
    check("bridge-nav: no full-page reload — the trusted shell stayed put",
      page.url() === parentUrlBefore, `url=${page.url()}`);

    // (c) An UNKNOWN-but-well-formed slug from the REAL frame is still rejected
    //     (resolved against the server's artifact table) — the reject path holds
    //     for the accepted message shape too, not just the pm-abuse garbage.
    if (navB) {
      const schemaBefore = await page.evaluate(() => window.__bridgeStats.rejectedSchema);
      const accBefore = await page.evaluate(() => window.__bridgeStats.accepted);
      await navB.evaluate(() => parent.postMessage({ type: "navigate", slug: "no-such-slug" }, "*"));
      await page.waitForTimeout(150);
      const schemaAfter = await page.evaluate(() => window.__bridgeStats.rejectedSchema);
      const accAfter = await page.evaluate(() => window.__bridgeStats.accepted);
      check("bridge-nav: navigate to an unknown slug is rejected (not in the artifact table)",
        schemaAfter === schemaBefore + 1 && accAfter === accBefore,
        `schema ${schemaBefore}->${schemaAfter} accepted ${accBefore}->${accAfter}`);

      // (c2) A KNOWN slug but with an EXTRA property is rejected by the exact
      //      schema — a hostile frame cannot smuggle a large/extra field through.
      const s2b = await page.evaluate(() => window.__bridgeStats.rejectedSchema);
      const a2b = await page.evaluate(() => window.__bridgeStats.accepted);
      await navB.evaluate(() =>
        parent.postMessage({ type: "navigate", slug: "index", padding: "A".repeat(50000) }, "*"));
      await page.waitForTimeout(120);
      const s2a = await page.evaluate(() => window.__bridgeStats.rejectedSchema);
      const a2a = await page.evaluate(() => window.__bridgeStats.accepted);
      check("bridge-nav: navigate with an EXTRA property is rejected (exact schema, not just type/slug)",
        s2a === s2b + 1 && a2a === a2b, `schema ${s2b}->${s2a} accepted ${a2b}->${a2a}`);

      // (c3) A transferred MessagePort is rejected (no covert back-channel).
      const s3b = await page.evaluate(() => window.__bridgeStats.rejectedSchema);
      const a3b = await page.evaluate(() => window.__bridgeStats.accepted);
      await navB.evaluate(() => {
        const ch = new MessageChannel();
        parent.postMessage({ type: "navigate", slug: "index" }, "*", [ch.port2]);
      });
      await page.waitForTimeout(120);
      const s3a = await page.evaluate(() => window.__bridgeStats.rejectedSchema);
      const a3a = await page.evaluate(() => window.__bridgeStats.accepted);
      check("bridge-nav: a transferred MessagePort is rejected (one-way bridge, no covert channel)",
        s3a === s3b + 1 && a3a === a3b, `schema ${s3b}->${s3a} accepted ${a3b}->${a3a}`);
    }

    // (d) Theme toggle re-themes the framed artifact via the parent→child channel.
    await page.click("#gp-theme-toggle").catch(() => {});
    await page.waitForTimeout(250);
    const themed = navB
      ? await navB.evaluate(() => document.documentElement.getAttribute("data-theme")).catch(() => null)
      : null;
    check("bridge-theme: shell toggle re-themes the artifact (data-theme applied via bridge)",
      themed === "light", `data-theme=${themed}`);

    // (d2) A theme message NOT from the parent (the artifact posts to itself) is
    //      ignored — bridge.js only trusts `event.source === window.parent`.
    if (navB) {
      await navB.evaluate(() => window.postMessage({ type: "theme", theme: "dark" }, "*"));
      await page.waitForTimeout(120);
      const stillLight = await navB
        .evaluate(() => document.documentElement.getAttribute("data-theme"))
        .catch(() => null);
      check("bridge-theme: a self-posted theme (wrong source) is ignored (stayed 'light', not 'dark')",
        stillLight === "light", `data-theme=${stillLight}`);
    }

    // (a) EXTERNAL link is NOT intercepted by the bridge — and, per the wrapped
    //     fragment's `<base target="_top">`, it breaks OUT of the null-origin sandbox
    //     to the TOP-LEVEL tab instead of navigating in-frame (issue
    //     hosted-interpage-link-refused: an in-frame nav would hit the target page's
    //     `x-frame-options: DENY` shell → "refused to connect"). If the bridge had
    //     (wrongly) intercepted it, it would `preventDefault` and the top would stay
    //     put with only the iframe swapping — so "the top navigated away from the
    //     shell to the link's real destination" proves BOTH non-interception and the
    //     top-level break-out. Fresh load so the tear-down can't taint earlier state.
    await page.goto(`${BASE}/demo/nav-a`, { waitUntil: "load" });
    const navA2 = await waitFrame("nav-a");
    if (navA2) await navA2.waitForSelector("#to-ext").catch(() => {});
    if (navA2) await navA2.evaluate(() => document.getElementById("to-ext").click());
    await page.waitForTimeout(600);
    const topAfterExt = page.url();
    check("bridge-nav: EXTERNAL link is not intercepted — it breaks out to the TOP-LEVEL tab (base target=_top)",
      !/\/demo\/nav-a(\?|#|$)/.test(topAfterExt),
      `top url=${topAfterExt}`);

    // (a2) A ROOT-RELATIVE (absolute-path) same-origin link is likewise NOT
    //      intercepted by the bridge (only path-relative links are). It too breaks
    //      out to the TOP level via `<base target="_top">`, loading the content route
    //      as a sandboxed top-level direct-open — NOT an in-frame bridge swap (which
    //      would keep the top at /demo/nav-a). Proof: the top URL is now the target
    //      content route itself.
    await page.goto(`${BASE}/demo/nav-a`, { waitUntil: "load" });
    const navA3 = await waitFrame("nav-a");
    if (navA3) await navA3.waitForSelector("#to-abs").catch(() => {});
    if (navA3) await navA3.evaluate(() => document.getElementById("to-abs").click());
    await page.waitForTimeout(600);
    const topAfterAbs = page.url();
    check("bridge-nav: ABSOLUTE-PATH link is not intercepted — it breaks out to the TOP-LEVEL content route (not a bridge swap)",
      /\/demo\/_c\/nav-b(\?|#|$)/.test(topAfterAbs), `top url=${topAfterAbs}`);
  }

  // ---------------------------------------------------------------------
  // TEST 8 — Wave 4 nav chrome, rendered in the TRUSTED parent. The parent
  // lists the space's artifacts and swaps the iframe in place (no full reload)
  // via the same validated navigate path. The critical trust-boundary assertion
  // is the INJECTION PROBE: the `inject` fixture carries a title that resolves to
  // raw hostile markup (`"><img onerror=…><script>…`), and the parent must render
  // it as inert TEXT — never executing, never breaking layout.
  // ---------------------------------------------------------------------
  {
    await page.goto(`${BASE}/demo/`, { waitUntil: "load" });
    await page.waitForSelector("#gp-nav a[data-slug]");

    // The nav lists the space's artifacts, each an <a data-slug>.
    const navSlugs = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#gp-nav a[data-slug]")).map((a) => a.getAttribute("data-slug")));
    check("nav-chrome: parent renders a nav listing the space's artifacts",
      navSlugs.length >= 2 && navSlugs.includes("index") && navSlugs.includes("inject"),
      `slugs=${JSON.stringify(navSlugs).slice(0, 160)}`);

    // Injection: the hostile-titled artifact is rendered as inert text. The
    // assertions inspect the WHOLE trusted chrome (not just the target anchor), so
    // a parser breakout that planted a sibling/overlay anywhere in the header would
    // be caught — not only the CSP-blocked inline-handler execution.
    const inj = await page.evaluate(() => {
      const a = document.querySelector('#gp-nav a[data-slug="inject"]');
      return {
        fired: window.__navInjectionFired === true,
        // No unexpected element node anywhere in the trusted chrome (nav is anchors
        // only; the header carries no img/script/style/iframe/form/meta/svg).
        strayNavEls: document.querySelectorAll("#gp-nav :not(a)").length,
        strayHeaderEls: document.querySelectorAll(
          "header.gp-chrome img, header.gp-chrome script, header.gp-chrome style, " +
          "header.gp-chrome iframe, header.gp-chrome form, header.gp-chrome meta, " +
          "header.gp-chrome svg, header.gp-chrome object").length,
        // No duplicate critical ids (an id-clobbering breakout would add one).
        dupIds: document.querySelectorAll("#gp-nav").length !== 1 ||
                document.querySelectorAll("#gp-title").length !== 1 ||
                document.querySelectorAll("#gp-artifact").length !== 1,
        anchorChildEls: a ? a.childElementCount : -1,
        // textContent carries the raw hostile string verbatim (proof it's TEXT,
        // not parsed markup): the `<img …>` survives only as characters.
        textHasRawMarkup: !!(a && /<img /i.test(a.textContent) && /onerror=/i.test(a.textContent)),
      };
    });
    check("nav-injection: hostile artifact title did NOT execute in the trusted parent",
      inj.fired === false, `__navInjectionFired=${inj.fired}`);
    check("nav-injection: no stray element nodes anywhere in the trusted chrome (textContent, not innerHTML)",
      inj.strayNavEls === 0 && inj.strayHeaderEls === 0 && inj.anchorChildEls === 0 && inj.dupIds === false,
      `strayNav=${inj.strayNavEls} strayHeader=${inj.strayHeaderEls} anchorChildren=${inj.anchorChildEls} dupIds=${inj.dupIds}`);
    check("nav-injection: the hostile markup survives only as inert TEXT in the nav label",
      inj.textHasRawMarkup === true, "textContent should contain the raw <img …> as characters");

    // Clicking a parent-nav entry swaps the iframe in place — no full reload.
    const frameHas = (slug) =>
      page.frames().some((f) => new RegExp(`/demo/_c/${slug}(\\?|$)`).test(f.url()));
    const parentUrlBefore = page.url();
    await page.evaluate(() => { window.__shellNotReloaded = true; }); // wiped by any full reload
    await page.click('#gp-nav a[data-slug="eval"]');
    await page.waitForFunction(
      () => document.getElementById("gp-artifact").getAttribute("src") &&
            /\/demo\/_c\/eval(\?|$)/.test(document.getElementById("gp-artifact").getAttribute("src")),
      { timeout: 4000 }).catch(() => {});
    check("nav-chrome: clicking a nav entry swapped the framed artifact in place",
      frameHas("eval"), "iframe src should be /demo/_c/eval");
    check("nav-chrome: parent nav navigation did NOT reload the trusted shell",
      (await page.evaluate(() => window.__shellNotReloaded === true)) && page.url() === parentUrlBefore,
      `sentinel survived, url=${page.url()}`);
    check("nav-chrome: the active nav entry is marked aria-current",
      await page.evaluate(() =>
        document.querySelector('#gp-nav a[data-slug="eval"]').getAttribute("aria-current") === "page"));
    check("nav-chrome: the previously-active entry cleared its aria-current (single active)",
      await page.evaluate(() =>
        document.querySelectorAll('#gp-nav a[aria-current="page"]').length === 1));
  }

  // ---------------------------------------------------------------------
  // TEST 9 — full-document cross-nav uses `target="_top"` (D1). A full document
  // gets no injected bridge; its author-written same-space link opts into a
  // user-activated TOP navigation. Clicking it must navigate the whole tab to the
  // sibling artifact's shell — the sanctioned top-nav path, not an iframe swap.
  // ---------------------------------------------------------------------
  {
    await page.goto(`${BASE}/demo/nav-full`, { waitUntil: "load" });
    const full = page.frames().find((f) => /\/demo\/_c\/nav-full(\?|$)/.test(f.url()));
    check("full-nav: full-document artifact is framed (served verbatim, no bridge)", !!full,
      full ? full.url() : "no nav-full frame");
    if (full) await full.waitForSelector("#to-a-top").catch(() => {});
    // A real user click (trusted activation) so allow-top-navigation-by-user-activation applies.
    const beforeUrl = page.url();
    if (full) await full.click("#to-a-top").catch(() => {});
    await page.waitForURL(/\/demo\/nav-a(\?|$)/, { timeout: 4000 }).catch(() => {});
    check("full-nav: target=_top link navigated the whole tab to the sibling's shell",
      /\/demo\/nav-a(\?|$)/.test(page.url()) && page.url() !== beforeUrl, `url=${page.url()}`);
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
