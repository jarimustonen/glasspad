(function() {
  'use strict';

  // --- Bootstrap with error handling ---
  var spec, datasets, container;
  try {
    var specEl = document.getElementById('glasspad-spec');
    var dataEl = document.getElementById('glasspad-data');
    container = document.getElementById('dashboard');
    if (!specEl || !dataEl || !container) throw new Error('Missing page elements');
    spec = JSON.parse(specEl.textContent);
    datasets = JSON.parse(dataEl.textContent);
    if (!spec || !Array.isArray(spec.sections)) throw new Error('Invalid spec');
  } catch (err) {
    console.error('Glasspad init failed:', err);
    var p = document.createElement('p');
    p.className = 'section-error';
    p.textContent = 'Dashboard failed to load: ' + err.message;
    (container || document.body).appendChild(p);
    return;
  }

  // --- Chart view registry (for filtering updates) ---
  var chartViews = {};

  // --- Utilities ---
  function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;')
      .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  function appendError(parent, msg) {
    var p = document.createElement('p');
    p.className = 'section-error';
    p.textContent = msg;
    parent.appendChild(p);
  }

  function formatCell(v) {
    if (v === null || v === undefined) return '';
    if (typeof v === 'boolean') return String(v);
    if (typeof v === 'number') return formatDecimal(v);
    return String(v);
  }

  function formatDecimal(n) {
    if (!isFinite(n)) return String(n);
    if (Math.trunc(n) === n) return Math.trunc(n).toLocaleString('en-US');
    return n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  }

  function formatCount(n) {
    return Math.trunc(n).toLocaleString('en-US');
  }

  function distinctKey(v) {
    if (v === null) return 'null';
    return typeof v + ':' + String(v);
  }

  function countDistinct(data, field) {
    var seen = {};
    for (var i = 0; i < data.length; i++) {
      var v = data[i][field];
      if (v !== null && v !== undefined) seen[distinctKey(v)] = true;
    }
    return Object.keys(seen).length;
  }

  function getMarkType(mark) {
    if (typeof mark === 'string') return mark;
    if (mark && typeof mark === 'object') return mark.type;
    return null;
  }

  function normalizeMark(mark) {
    if (typeof mark === 'string') return { type: mark, tooltip: true };
    if (mark && typeof mark === 'object') {
      var out = {};
      for (var k in mark) out[k] = mark[k];
      if (out.tooltip === undefined) out.tooltip = true;
      return out;
    }
    return mark;
  }

  function isHorizontalBar(cfg) {
    if (getMarkType(cfg.mark) !== 'bar') return false;
    var enc = cfg.encoding || {};
    var xIsQuant = enc.x && (enc.x.type === 'quantitative' || enc.x.aggregate);
    var yIsCat = enc.y && (enc.y.type === 'nominal' || enc.y.type === 'ordinal');
    return xIsQuant && yIsCat;
  }

  // --- Data resolution ---
  function getDataResult(section) {
    if (section.source) {
      if (!(section.source in datasets)) {
        return { ok: false, error: 'Unknown dataset: ' + section.source };
      }
      return { ok: true, data: datasets[section.source] };
    }
    if (section.inline_data) {
      return { ok: true, data: section.inline_data };
    }
    return { ok: false, error: 'No data source configured' };
  }

  // --- Section layout metadata (computed once, used for rendering + span decisions) ---
  function getSectionLayoutMeta(section, dataResult) {
    var meta = { spanFull: false, categories: 0, shouldCollapse: false };
    if (section.type === 'table') { meta.spanFull = true; return meta; }
    if (section.type === 'chart' && section.chart && isHorizontalBar(section.chart)) {
      if (dataResult.ok && dataResult.data) {
        var yField = (section.chart.encoding.y || {}).field;
        if (yField) {
          meta.categories = countDistinct(dataResult.data, yField);
          if (meta.categories > 8) meta.spanFull = true;
          if (meta.categories > CHART_COLLAPSE_THRESHOLD) meta.shouldCollapse = true;
        }
      }
    }
    return meta;
  }

  // --- Collapse toggle (shared by charts and tables) ---
  function addCollapseToggle(actionsEl, wrapper, showAllText, onToggle) {
    var expanded = false;
    var wrapperId = wrapper.id || ('collapse-' + Math.random().toString(36).slice(2, 8));
    wrapper.id = wrapperId;

    var topLink = document.createElement('button');
    topLink.className = 'collapse-top-link';
    topLink.textContent = 'Show less';
    topLink.style.display = 'none';
    topLink.setAttribute('aria-controls', wrapperId);
    topLink.setAttribute('aria-expanded', 'false');
    actionsEl.appendChild(topLink);

    var bottomBtn = document.createElement('button');
    bottomBtn.className = 'table-show-more';
    bottomBtn.textContent = showAllText;
    bottomBtn.setAttribute('aria-controls', wrapperId);
    bottomBtn.setAttribute('aria-expanded', 'false');

    function toggle() {
      expanded = !expanded;
      var expStr = String(expanded);
      bottomBtn.setAttribute('aria-expanded', expStr);
      topLink.setAttribute('aria-expanded', expStr);
      if (expanded) {
        wrapper.classList.remove('collapsed');
        bottomBtn.textContent = 'Show less';
        topLink.style.display = '';
      } else {
        wrapper.classList.add('collapsed');
        bottomBtn.textContent = showAllText;
        topLink.style.display = 'none';
        wrapper.parentElement.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
      if (onToggle) onToggle(expanded);
    }

    bottomBtn.addEventListener('click', toggle);
    topLink.addEventListener('click', toggle);

    // Insert bottom button after wrapper
    if (wrapper.nextSibling) {
      wrapper.parentNode.insertBefore(bottomBtn, wrapper.nextSibling);
    } else {
      wrapper.parentNode.appendChild(bottomBtn);
    }
  }

  // --- Create section card with stable header structure ---
  function createSectionCard(section, index) {
    var card = document.createElement('div');
    card.className = 'section-card';
    card.id = section.id || ('section-' + index);

    var header = document.createElement('div');
    header.className = 'section-header';

    var h3 = document.createElement('h3');
    h3.textContent = section.title;
    header.appendChild(h3);

    var actions = document.createElement('div');
    actions.className = 'section-header-actions';
    header.appendChild(actions);

    var body = document.createElement('div');
    body.className = 'section-body';

    card.appendChild(header);
    card.appendChild(body);

    return { card: card, body: body, actions: actions };
  }

  // --- Section rendering ---
  function renderSection(section, index, layoutMeta) {
    var s = createSectionCard(section, index);
    var dataResult = getDataResult(section);

    try {
      switch (section.type) {
        case 'chart':
          renderChart(s, section, dataResult, index, layoutMeta);
          break;
        case 'table':
          renderTable(s, section, dataResult);
          break;
        case 'stats':
          renderStats(s, section, dataResult);
          break;
        case 'list':
          appendError(s.body, 'List rendering coming soon');
          break;
        default:
          appendError(s.body, 'Unknown section type: ' + String(section.type));
      }
    } catch (e) {
      appendError(s.body, 'Render error: ' + e.message);
      console.error('Section "' + section.title + '":', e);
    }

    return s.card;
  }

  // --- Chart ---
  var CHART_COLLAPSED_HEIGHT = 350;
  var CHART_COLLAPSE_THRESHOLD = 10;

  function renderChart(s, section, dataResult, index, layoutMeta) {
    var cfg = section.chart;
    if (!cfg) { appendError(s.body, 'No chart config'); return; }
    if (!dataResult.ok) { appendError(s.body, dataResult.error); return; }

    var sectionKey = section.id || ('section-' + index);
    var data = dataResult.data || [];
    var mark = normalizeMark(cfg.mark);
    var markType = getMarkType(cfg.mark);

    // Dynamic height for horizontal bar charts
    var height = 300;
    if (layoutMeta.categories > 0) {
      height = Math.max(200, layoutMeta.categories * 22 + 60);
    }

    var wrapper = document.createElement('div');
    wrapper.className = 'chart-container';
    if (layoutMeta.shouldCollapse) {
      wrapper.classList.add('collapsed');
      wrapper.style.maxHeight = CHART_COLLAPSED_HEIGHT + 'px';
    }

    var div = document.createElement('div');
    wrapper.appendChild(div);
    s.body.appendChild(wrapper);

    var vlSpec = {
      '$schema': 'https://vega.github.io/schema/vega-lite/v5.json',
      width: 'container',
      height: height,
      mark: mark,
      data: { values: data },
      encoding: cfg.encoding || {}
    };

    if (markType === 'arc') {
      delete vlSpec.width;
      vlSpec.height = 300;
      vlSpec.view = { stroke: null };
    }

    vegaEmbed(div, vlSpec, { actions: false, renderer: 'svg' })
      .then(function(result) {
        chartViews[sectionKey] = result.view;
      })
      .catch(function(err) {
        console.error('Chart "' + section.title + '":', err);
        appendError(div, 'Chart error: ' + err.message);
      });

    if (layoutMeta.shouldCollapse) {
      addCollapseToggle(s.actions, wrapper, 'Show all ' + layoutMeta.categories + ' categories', function(exp) {
        wrapper.style.maxHeight = exp ? '' : CHART_COLLAPSED_HEIGHT + 'px';
      });
    }
  }

  // --- Table ---
  var INITIAL_ROWS = 10;
  var MAX_ROWS = 1000;

  function detectSortType(data, field) {
    for (var i = 0; i < Math.min(data.length, 50); i++) {
      var v = data[i][field];
      if (v === null || v === undefined) continue;
      if (typeof v === 'number') return 'number';
      if (typeof v === 'boolean') return 'boolean';
      if (typeof v === 'string') {
        if (/^\d{4}-\d{2}-\d{2}/.test(v)) return 'temporal';
        return 'string';
      }
    }
    return 'string';
  }

  function compareValues(a, b, sortType, ascending) {
    var aNull = (a === null || a === undefined || a === '');
    var bNull = (b === null || b === undefined || b === '');
    if (aNull && bNull) return 0;
    if (aNull) return 1;
    if (bNull) return -1;

    var result = 0;
    switch (sortType) {
      case 'number':
        var an = Number(a), bn = Number(b);
        var aBad = !isFinite(an), bBad = !isFinite(bn);
        if (aBad && bBad) result = 0;
        else if (aBad) result = 1;
        else if (bBad) result = -1;
        else result = an - bn;
        break;
      case 'temporal':
        var at = Date.parse(a), bt = Date.parse(b);
        if (!isNaN(at) && !isNaN(bt)) { result = at - bt; }
        else { result = String(a).localeCompare(String(b)); }
        break;
      case 'boolean':
        result = (a === b) ? 0 : a ? 1 : -1;
        break;
      default:
        result = String(a).localeCompare(String(b));
    }
    return ascending ? result : -result;
  }

  function renderTable(s, section, dataResult) {
    var cfg = section.table;
    if (!cfg || !cfg.columns) { appendError(s.body, 'No table config'); return; }
    if (!dataResult.ok) { appendError(s.body, dataResult.error); return; }
    var allData = dataResult.data;
    if (!allData || allData.length === 0) { appendError(s.body, 'No data'); return; }

    var totalRows = Math.min(allData.length, MAX_ROWS);
    var sourceData = allData.slice(0, totalRows);
    var displayData = sourceData.slice();

    var sortTypes = {};
    for (var c = 0; c < cfg.columns.length; c++) {
      var col = cfg.columns[c];
      sortTypes[col.field] = col.sort || detectSortType(sourceData, col.field);
    }

    var sortState = null;

    // Build stable table structure
    var wrapper = document.createElement('div');
    wrapper.className = 'table-wrapper';
    if (totalRows > INITIAL_ROWS) wrapper.classList.add('collapsed');

    var table = document.createElement('table');
    var thead = document.createElement('thead');
    var tbody = document.createElement('tbody');
    table.appendChild(thead);
    table.appendChild(tbody);
    wrapper.appendChild(table);
    s.body.appendChild(wrapper);

    // Build thead once
    function buildThead() {
      var tr = document.createElement('tr');
      for (var c = 0; c < cfg.columns.length; c++) {
        var col = cfg.columns[c];
        var th = document.createElement('th');
        th.setAttribute('scope', 'col');

        var isActive = sortState && sortState.field === col.field;
        var ariaSort = 'none';
        if (isActive) ariaSort = sortState.ascending ? 'ascending' : 'descending';
        th.setAttribute('aria-sort', ariaSort);
        if (isActive) th.className = sortState.ascending ? 'sort-asc' : 'sort-desc';
        if (col.width) th.style.width = col.width + 'px';

        var btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'table-sort-button';
        btn.setAttribute('data-field', col.field);
        btn.textContent = col.title || col.field;

        var indicator = document.createElement('span');
        indicator.className = 'sort-indicator';
        if (isActive) {
          indicator.textContent = sortState.ascending ? ' \u25B2' : ' \u25BC';
        }
        btn.appendChild(indicator);
        btn.addEventListener('click', onHeaderClick);

        th.appendChild(btn);
        tr.appendChild(th);
      }
      thead.innerHTML = '';
      thead.appendChild(tr);
    }

    // Rebuild tbody only
    function rebuildTbody() {
      if (sortState) {
        var sf = sortState.field;
        var sa = sortState.ascending;
        var st = sortTypes[sf] || 'string';
        displayData.sort(function(a, b) {
          return compareValues(a[sf], b[sf], st, sa);
        });
      } else {
        displayData = sourceData.slice();
      }

      var html = '';
      for (var r = 0; r < displayData.length; r++) {
        html += '<tr>';
        for (var c = 0; c < cfg.columns.length; c++) {
          var val = formatCell(displayData[r][cfg.columns[c].field]);
          html += '<td title="' + esc(val) + '">' + esc(val) + '</td>';
        }
        html += '</tr>';
      }
      tbody.innerHTML = html;
    }

    function onHeaderClick(e) {
      var field = e.currentTarget.getAttribute('data-field');
      if (!field) return;
      if (!sortState || sortState.field !== field) {
        sortState = { field: field, ascending: true };
      } else if (sortState.ascending) {
        sortState.ascending = false;
      } else {
        sortState = null;
      }
      buildThead();
      rebuildTbody();
    }

    buildThead();
    rebuildTbody();

    if (totalRows > INITIAL_ROWS) {
      var showAllText = 'Show all ' + totalRows + ' rows';
      if (allData.length > MAX_ROWS) showAllText += ' (of ' + allData.length + ' total)';
      addCollapseToggle(s.actions, wrapper, showAllText);
    }
  }

  // --- Stats ---
  function renderStats(s, section, dataResult) {
    if (section.stats && section.stats.items) {
      if (!dataResult.ok) { appendError(s.body, dataResult.error); return; }
      renderAggregateStats(s.body, section.stats.items, dataResult.data || []);
      return;
    }
    if (section.inline_data) {
      renderInlineStats(s.body, section.inline_data);
      return;
    }
    appendError(s.body, 'No stats config');
  }

  function renderInlineStats(parent, rows) {
    var grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (var i = 0; i < rows.length; i++) {
      grid.appendChild(statCard(formatCell(rows[i].label), formatCell(rows[i].value)));
    }
    parent.appendChild(grid);
  }

  function renderAggregateStats(parent, items, data) {
    var grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (var i = 0; i < items.length; i++) {
      grid.appendChild(statCard(items[i].label, computeAggregate(items[i], data)));
    }
    parent.appendChild(grid);
  }

  function statCard(label, value) {
    var sc = document.createElement('div');
    sc.className = 'stat-card';
    var val = document.createElement('div');
    val.className = 'stat-value';
    val.textContent = value;
    var lbl = document.createElement('div');
    lbl.className = 'stat-label';
    lbl.textContent = label;
    sc.appendChild(val);
    sc.appendChild(lbl);
    return sc;
  }

  function computeAggregate(item, data) {
    var filtered = data;
    var whereClause = item.where || item.where_clause;
    if (whereClause) {
      var keys = Object.keys(whereClause);
      filtered = data.filter(function(row) {
        for (var i = 0; i < keys.length; i++) {
          if (!valueEquals(row[keys[i]], whereClause[keys[i]])) return false;
        }
        return true;
      });
    }

    var agg = item.aggregate;
    var field = item.field;

    if (agg === 'count') return formatCount(filtered.length);
    if (!field) return '\u26a0 missing field';

    if (agg === 'distinct') return formatCount(countDistinct(filtered, field));

    var nums = [];
    for (var j = 0; j < filtered.length; j++) {
      var n = filtered[j][field];
      if (typeof n === 'number' && isFinite(n)) nums.push(n);
    }
    if (nums.length === 0) return '\u2014';

    if (agg === 'sum') {
      var sum = 0;
      for (var si = 0; si < nums.length; si++) sum += nums[si];
      return formatDecimal(sum);
    }
    if (agg === 'avg') {
      var total = 0;
      for (var ai = 0; ai < nums.length; ai++) total += nums[ai];
      return formatDecimal(total / nums.length);
    }
    if (agg === 'min') {
      var min = nums[0];
      for (var mi = 1; mi < nums.length; mi++) if (nums[mi] < min) min = nums[mi];
      return formatDecimal(min);
    }
    if (agg === 'max') {
      var max = nums[0];
      for (var xi = 1; xi < nums.length; xi++) if (nums[xi] > max) max = nums[xi];
      return formatDecimal(max);
    }

    return '\u26a0 unknown: ' + agg;
  }

  function valueEquals(a, b) {
    if (a === b) return true;
    if (a == null || b == null) return a === b;
    return false;
  }

  // --- Render all sections ---
  spec.sections.forEach(function(section, i) {
    var dataResult = getDataResult(section);
    var layoutMeta = getSectionLayoutMeta(section, dataResult);
    var card = renderSection(section, i, layoutMeta);
    if (layoutMeta.spanFull) card.classList.add('span-full');
    container.appendChild(card);
  });

})();
