/* Glasspad chart helper — /_gp/v1/charts.js
 *
 * A thin `gp.chart(elOrSelector, spec)` over Vega-Lite, served INSIDE the
 * null-origin artifact sandbox. It lazily loads the pinned Vega / Vega-Lite /
 * Vega-Embed bundles from the SAME named host (`/_gp/v1/*`), which the frozen
 * Wave-1 artifact `script-src` permits (design.md §4). Vega-Lite compiles its
 * expression language with the `Function` constructor, which is why that policy
 * includes `'unsafe-eval'` — charts.js relies on that and widens nothing.
 *
 * Specs must be SELF-CONTAINED: the artifact sandbox sets `connect-src 'none'`,
 * so Vega cannot fetch `data.url`, external images, or remote fonts — pass data
 * inline via `data.values`. A URL-backed spec fails at load, not in charts.js.
 *
 * Theme comes from the `--gp-*` tokens in base.css: gp.chart reads them at
 * render time and rebuilds the Vega config, so the same spec renders correctly
 * in Glass Light and Glass Dark. When the surrounding `data-theme` changes
 * (theme toggle), tracked charts re-render automatically.
 *
 * API:
 *   gp.chart(elOrSelector, vegaLiteSpec[, opts]) -> Promise<vegaEmbedResult>
 *   gp.themeConfig()                             -> Vega config object
 *   opts: { actions=false, renderer="svg", theme=true, reactiveTheme=true }
 */
(function () {
  "use strict";

  var gp = (window.gp = window.gp || {});

  var VERSION = "v1";
  var BASE = "/_gp/" + VERSION + "/";
  // Load order matters: vega-lite needs the vega global; vega-embed needs both.
  var DEPS = ["vega.min.js", "vega-lite.min.js", "vega-embed.min.js"];
  var FONT =
    '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';

  var loadPromise = null;

  function loadScript(url) {
    return new Promise(function (resolve, reject) {
      var s = document.createElement("script");
      s.src = url;
      // Preserve execution order for dynamically-inserted scripts.
      s.async = false;
      s.onload = function () {
        resolve();
      };
      s.onerror = function () {
        // Drop the dead node so a retry doesn't leave orphans behind.
        if (s.parentNode) s.parentNode.removeChild(s);
        reject(new Error("gp.chart: failed to load " + url));
      };
      (document.head || document.documentElement).appendChild(s);
    });
  }

  function vegaReady() {
    // vega-embed alone is not enough — an artifact could inline the wrapper
    // without the base libs. Require the whole stack before short-circuiting.
    return !!(window.vega && window.vegaLite && window.vegaEmbed);
  }

  // Load the Vega stack once, sequentially, from the named host. Resolves
  // immediately if the whole stack is already present (an artifact may inline
  // it). On failure the cached promise is cleared so a later call can retry
  // (e.g. after a transient loopback hiccup).
  function ensureVega() {
    if (vegaReady()) return Promise.resolve();
    if (loadPromise) return loadPromise;
    loadPromise = DEPS.reduce(function (chain, dep) {
      return chain.then(function () {
        return loadScript(BASE + dep);
      });
    }, Promise.resolve());
    loadPromise.catch(function () {
      loadPromise = null;
    });
    return loadPromise;
  }

  function cssVar(name) {
    try {
      return getComputedStyle(document.documentElement)
        .getPropertyValue(name)
        .trim();
    } catch (e) {
      return "";
    }
  }

  // Build a Vega config from the current `--gp-*` tokens. Mirrors the theme
  // mapping in src/client/dashboard.js so artifacts and dashboards match.
  gp.themeConfig = function themeConfig() {
    var text = cssVar("--gp-text") || "#1a1b25";
    var muted = cssVar("--gp-text-muted") || "#6b7280";
    var grid = cssVar("--gp-chart-grid") || "#e5e7eb";
    var axis = cssVar("--gp-chart-axis") || muted;
    var catStr = cssVar("--gp-chart-cat");
    var palette = catStr
      ? catStr
          .split(",")
          .map(function (c) {
            return c.trim();
          })
          .filter(Boolean)
      : null;

    var cfg = {
      background: "transparent",
      view: { stroke: null },
      axis: {
        labelColor: muted,
        titleColor: muted,
        tickColor: axis,
        gridColor: grid,
        domainColor: grid,
        labelFont: FONT,
        titleFont: FONT,
        labelFontSize: 11,
        titleFontSize: 12,
      },
      legend: {
        labelColor: muted,
        titleColor: muted,
        labelFont: FONT,
        titleFont: FONT,
      },
      title: {
        color: text,
        font: FONT,
        fontSize: 14,
      },
    };
    if (palette && palette.length) {
      cfg.range = { category: palette };
    }
    return cfg;
  };

  function resolveEl(elOrSelector) {
    if (typeof elOrSelector === "string") {
      return document.querySelector(elOrSelector);
    }
    return elOrSelector;
  }

  function showError(el, message) {
    if (!el) return;
    var p = document.createElement("div");
    p.className = "gp-chart-error";
    p.textContent = "Chart error: " + message;
    el.textContent = "";
    el.appendChild(p);
  }

  function finalizeView(el) {
    if (el && el.__gpView && typeof el.__gpView.finalize === "function") {
      try {
        el.__gpView.finalize();
      } catch (e) {
        /* view already torn down */
      }
    }
  }

  // --- Reactive theme: re-render tracked charts on theme change --------------
  //
  // A chart is tracked at most once per container (last spec wins). On a theme
  // change we prune detached containers (finalizing their Vega views so their
  // listeners/timers don't leak) and re-render the rest.

  var tracked = [];
  var themeObserver = null;
  var mediaHooked = false;

  function track(el, spec, opts) {
    // De-dupe by element: a re-render with a new spec replaces the old entry.
    tracked = tracked.filter(function (entry) {
      return entry.el !== el && entry.el && entry.el.isConnected === true;
    });
    tracked.push({ el: el, spec: spec, opts: opts });
  }

  function rerenderTracked() {
    tracked = tracked.filter(function (entry) {
      if (!entry.el || entry.el.isConnected !== true) {
        finalizeView(entry.el);
        return false;
      }
      // Keep the existing chart visible if a re-render fails (don't wipe it).
      render(entry.el, entry.spec, entry.opts).catch(function () {});
      return true;
    });
  }

  function startThemeObserver() {
    if (!themeObserver && typeof MutationObserver !== "undefined") {
      themeObserver = new MutationObserver(rerenderTracked);
      themeObserver.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["data-theme"],
      });
    }
    // With data-theme="auto" the tokens follow prefers-color-scheme, which the
    // attribute observer never sees. Re-render on OS scheme change too, but
    // only while the document is actually in auto mode.
    if (!mediaHooked && window.matchMedia) {
      mediaHooked = true;
      var mql = window.matchMedia("(prefers-color-scheme: dark)");
      var onScheme = function () {
        if (document.documentElement.getAttribute("data-theme") === "auto") {
          rerenderTracked();
        }
      };
      if (mql.addEventListener) mql.addEventListener("change", onScheme);
      else if (mql.addListener) mql.addListener(onScheme); // legacy WebKit
    }
  }

  function render(el, spec, opts) {
    var actions = opts.actions === undefined ? false : opts.actions;
    // The "Open in Vega Editor" action POSTs a form to an external host, which
    // `form-action 'none'` blocks — ship it disabled rather than a dead button.
    if (actions === true) {
      actions = { export: true, source: true, compiled: true, editor: false };
    }
    var embedOpts = {
      actions: actions,
      renderer: opts.renderer || "svg",
    };
    if (opts.theme === false) {
      // Caller opts out of Glasspad theming; still merge any spec.config.
      if (spec.config) embedOpts.config = spec.config;
    } else {
      embedOpts.config = mergeConfig(gp.themeConfig(), spec.config);
    }
    // Tear down any prior view on this container before re-embedding.
    finalizeView(el);
    return window.vegaEmbed(el, spec, embedOpts).then(function (result) {
      // Stash the view so callers can update data / listen to signals.
      el.__gpView = result.view;
      return result;
    });
  }

  function isPlainObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v);
  }

  // Deep-merge the caller's spec.config OVER the theme config, so an artifact
  // can override an individual value (e.g. `axis.labelFontSize`) without losing
  // the rest of the themed block. Nested plain objects merge recursively;
  // arrays and scalars replace.
  function mergeConfig(base, override) {
    if (!isPlainObject(override)) return base;
    var out = {};
    var k;
    for (k in base) out[k] = base[k];
    for (k in override) {
      if (isPlainObject(base[k]) && isPlainObject(override[k])) {
        out[k] = mergeConfig(base[k], override[k]);
      } else {
        out[k] = override[k];
      }
    }
    return out;
  }

  /**
   * Render a Vega-Lite spec into a container.
   * @param {string|Element} elOrSelector  target container (or CSS selector)
   * @param {object} spec                  a Vega-Lite specification
   * @param {object} [opts]                { actions, renderer, theme, reactiveTheme }
   * @returns {Promise<object>}            the vega-embed result ({ view, spec, ... })
   */
  gp.chart = function chart(elOrSelector, spec, opts) {
    opts = opts || {};
    return ensureVega().then(function () {
      var el = resolveEl(elOrSelector);
      if (!el) {
        throw new Error("container not found: " + elOrSelector);
      }
      if (!spec || typeof spec !== "object") {
        throw new Error("spec must be a Vega-Lite object");
      }
      if (opts.reactiveTheme !== false && opts.theme !== false) {
        track(el, spec, opts);
        startThemeObserver();
      }
      return render(el, spec, opts).catch(function (err) {
        showError(el, err && err.message ? err.message : String(err));
        throw err;
      });
    });
  };
})();
