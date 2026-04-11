(function() {
  var html = document.documentElement;
  var btn = document.getElementById('theme-toggle');
  var specDefault = html.getAttribute('data-theme') || 'auto';

  // Resolve what "auto" means right now
  function systemTheme() {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function resolvedTheme(t) {
    return t === 'auto' ? systemTheme() : t;
  }

  // On load: check localStorage, fall back to spec default
  var stored = null;
  try { stored = localStorage.getItem('glasspad-theme'); } catch(e) {}
  var current = stored || specDefault;
  html.setAttribute('data-theme', current);

  function updateButton() {
    var resolved = resolvedTheme(current);
    // Sun for light (click to go dark), Moon for dark (click to go light)
    btn.textContent = resolved === 'dark' ? '\u2600' : '\u263E';
    btn.title = resolved === 'dark' ? 'Switch to light theme' : 'Switch to dark theme';
  }

  updateButton();

  btn.addEventListener('click', function() {
    var resolved = resolvedTheme(current);
    current = resolved === 'dark' ? 'light' : 'dark';
    html.setAttribute('data-theme', current);
    try { localStorage.setItem('glasspad-theme', current); } catch(e) {}
    updateButton();
    // Re-render charts with new theme colors
    if (typeof window.__glasspadOnThemeChange === 'function') {
      window.__glasspadOnThemeChange();
    }
  });

  // Listen for system theme changes when in auto mode
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function() {
    if (current === 'auto') {
      updateButton();
    }
  });
})();
