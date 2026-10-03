/* Mermaid support for Markdown `mermaid` fences. Runs only in artifact frames.
 * The source is escaped by the Markdown renderer and kept as a readable fallback.
 * Both this loader and the pinned bundle are served locally; CSP is unchanged.
 */
(function () {
  "use strict";
  var nodes = Array.from(document.querySelectorAll("[data-gp-mermaid]"));
  if (!nodes.length) return;
  var sources = nodes.map(function (node) { return node.textContent; });
  // Resolve relative to this script, not the page: static builds may live
  // below a path prefix, while hosted artifacts use the root /_gp/v1/ path.
  var base = new URL(".", document.currentScript.src);
  var bundle = document.createElement("script");
  bundle.src = new URL("mermaid.min.js", base).href;
  bundle.onload = function () {
    var mermaid = window.__gpMermaid;
    if (!mermaid) return;
    var pending = Promise.resolve();
    var requested = 0;
    var sequence = 0;
    function render() {
      var ticket = ++requested;
      // Mermaid's configuration is global. Serialize render requests so a theme
      // change cannot race an older render and leave diagrams in the wrong theme.
      pending = pending.catch(function () {}).then(async function () {
        if (ticket !== requested) return;
        var dark = document.documentElement.getAttribute("data-theme") === "dark" ||
          (document.documentElement.getAttribute("data-theme") !== "light" &&
           matchMedia("(prefers-color-scheme: dark)").matches);
        var styles = getComputedStyle(document.documentElement);
        var token = function (key) { return styles.getPropertyValue("--gp-" + key).trim(); };
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          theme: "base",
          themeVariables: {
            background: token("bg"), primaryColor: token("surface-alt"),
            primaryTextColor: token("text"), primaryBorderColor: token("border-strong"),
            lineColor: token("text-secondary"), secondaryColor: token("surface"),
            tertiaryColor: token("surface-alt"), textColor: token("text"),
            mainBkg: token("surface-alt"), noteBkgColor: token("surface-alt"),
            noteTextColor: token("text"), actorBkg: token("surface"),
            actorTextColor: token("text"), actorBorder: token("border-strong"),
            signalColor: token("text-secondary"), labelTextColor: token("text"),
            darkMode: dark
          }
        });
        for (var i = 0; i < nodes.length; i++) {
          try {
            // IDs are unique across rerenders; multiple diagrams can share a page.
            var result = await mermaid.render("gp-mermaid-" + (++sequence), sources[i]);
            if (ticket !== requested) return;
            nodes[i].innerHTML = result.svg;
            if (result.bindFunctions) result.bindFunctions(nodes[i]);
            nodes[i].classList.add("gp-mermaid-ready");
          } catch (error) {
            nodes[i].textContent = sources[i];
            nodes[i].classList.remove("gp-mermaid-ready");
            console.warn("Mermaid diagram could not be rendered:", error);
          }
        }
      });
    }
    new MutationObserver(function () { render(); }).observe(document.documentElement, {
      attributes: true, attributeFilter: ["data-theme"]
    });
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", function () {
      if (document.documentElement.getAttribute("data-theme") === "auto") render();
    });
    render();
  };
  bundle.onerror = function () { console.warn("Mermaid bundle could not be loaded"); };
  document.head.appendChild(bundle);
})();
