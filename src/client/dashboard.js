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

  // --- Chart view registry (for future filtering updates) ---
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
    if (Math.trunc(n) === n) {
      return Math.trunc(n).toLocaleString('en-US');
    }
    return n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  }

  function formatCount(n) {
    return Math.trunc(n).toLocaleString('en-US');
  }

  // --- Collapse toggle (shared by charts and tables) ---
  // Bottom "Show all" button + small "Show less" link next to section title
  function addCollapseToggle(card, wrapper, showAllText, onToggle) {
    var expanded = false;

    // Find the h3 in this card and wrap it in a flex header with "show less" link
    var h3 = card.querySelector('h3');
    var header = document.createElement('div');
    header.className = 'section-header';
    h3.parentNode.insertBefore(header, h3);
    header.appendChild(h3);

    var topLink = document.createElement('button');
    topLink.className = 'collapse-top-link';
    topLink.textContent = 'Show less';
    topLink.style.display = 'none';
    header.appendChild(topLink);

    // Bottom "show all"
    var bottomBtn = document.createElement('button');
    bottomBtn.className = 'table-show-more';
    bottomBtn.textContent = showAllText;

    function toggle() {
      expanded = !expanded;
      if (expanded) {
        wrapper.classList.remove('collapsed');
        bottomBtn.textContent = 'Show less';
        topLink.style.display = '';
      } else {
        wrapper.classList.add('collapsed');
        bottomBtn.textContent = showAllText;
        topLink.style.display = 'none';
        card.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
      if (onToggle) onToggle(expanded);
    }

    bottomBtn.addEventListener('click', toggle);
    topLink.addEventListener('click', toggle);

    if (wrapper.nextSibling) {
      card.insertBefore(bottomBtn, wrapper.nextSibling);
    } else {
      card.appendChild(bottomBtn);
    }
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

  // --- Count distinct values for a field in data ---
  function countDistinct(data, field) {
    var seen = {};
    for (var i = 0; i < data.length; i++) {
      var v = data[i][field];
      if (v !== null && v !== undefined) seen[String(v)] = true;
    }
    return Object.keys(seen).length;
  }

  // --- Detect if chart is horizontal bar (x is quantitative/aggregate, y is categorical) ---
  function isHorizontalBar(cfg) {
    if (cfg.mark !== 'bar') return false;
    var enc = cfg.encoding || {};
    var xIsQuant = enc.x && (enc.x.type === 'quantitative' || enc.x.aggregate);
    var yIsCat = enc.y && (enc.y.type === 'nominal' || enc.y.type === 'ordinal');
    return xIsQuant && yIsCat;
  }

  // --- Section rendering ---
  function renderSection(section, index) {
    var card = document.createElement('div');
    card.className = 'section-card';
    card.id = section.id || ('section-' + index);

    var h3 = document.createElement('h3');
    h3.textContent = section.title;
    card.appendChild(h3);

    var dataResult = getDataResult(section);

    try {
      switch (section.type) {
        case 'chart':
          renderChart(card, section, dataResult, index);
          break;
        case 'table':
          renderTable(card, section, dataResult);
          break;
        case 'stats':
          renderStats(card, section, dataResult);
          break;
        case 'list':
          appendError(card, 'List rendering coming soon');
          break;
        default:
          appendError(card, 'Unknown section type: ' + String(section.type));
      }
    } catch (e) {
      appendError(card, 'Render error: ' + e.message);
      console.error('Section "' + section.title + '":', e);
    }

    return card;
  }

  // --- Chart ---
  var CHART_COLLAPSED_HEIGHT = 350; // px, initial visible height for tall charts
  var CHART_COLLAPSE_THRESHOLD = 10; // categories before collapsing

  function renderChart(card, section, dataResult, index) {
    var cfg = section.chart;
    if (!cfg) { appendError(card, 'No chart config'); return; }

    var sectionKey = section.id || ('section-' + index);
    var data = dataResult.ok ? dataResult.data : [];
    var mark = cfg.mark === 'arc' ? { type: 'arc', tooltip: true } : cfg.mark;

    // Calculate dynamic height for horizontal bar charts
    var height = 300;
    var categories = 0;
    var shouldCollapse = false;
    if (isHorizontalBar(cfg) && data.length > 0) {
      var yField = (cfg.encoding.y || {}).field;
      if (yField) {
        categories = countDistinct(data, yField);
        height = Math.max(200, categories * 22 + 60);
        shouldCollapse = categories > CHART_COLLAPSE_THRESHOLD;
      }
    }

    var wrapper = document.createElement('div');
    wrapper.className = 'chart-container';
    if (shouldCollapse) {
      wrapper.classList.add('collapsed');
      wrapper.style.maxHeight = CHART_COLLAPSED_HEIGHT + 'px';
    }

    var div = document.createElement('div');
    wrapper.appendChild(div);
    card.appendChild(wrapper);

    var vlSpec = {
      '$schema': 'https://vega.github.io/schema/vega-lite/v5.json',
      width: 'container',
      height: height,
      mark: mark,
      data: { values: data },
      encoding: cfg.encoding || {}
    };

    if (cfg.mark === 'arc') {
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

    if (shouldCollapse) {
      addCollapseToggle(card, wrapper, 'Show all ' + categories + ' categories', function(exp) {
        if (exp) {
          wrapper.style.maxHeight = '';
        } else {
          wrapper.style.maxHeight = CHART_COLLAPSED_HEIGHT + 'px';
        }
      });
    }
  }

  // --- Table ---
  var INITIAL_ROWS = 10;
  var MAX_ROWS = 1000;

  // Detect sort type for a column from data values
  function detectSortType(data, field) {
    for (var i = 0; i < Math.min(data.length, 50); i++) {
      var v = data[i][field];
      if (v === null || v === undefined) continue;
      if (typeof v === 'number') return 'number';
      if (typeof v === 'boolean') return 'boolean';
      if (typeof v === 'string') {
        // ISO-8601 date pattern
        if (/^\d{4}-\d{2}-\d{2}/.test(v)) return 'temporal';
        return 'string';
      }
    }
    return 'string';
  }

  // Compare two values for sorting. Nulls always last.
  function compareValues(a, b, sortType, ascending) {
    var aNull = (a === null || a === undefined || a === '');
    var bNull = (b === null || b === undefined || b === '');
    if (aNull && bNull) return 0;
    if (aNull) return 1;  // nulls last regardless of direction
    if (bNull) return -1;

    var result = 0;
    switch (sortType) {
      case 'number':
        result = (Number(a) || 0) - (Number(b) || 0);
        break;
      case 'temporal':
        result = String(a) < String(b) ? -1 : String(a) > String(b) ? 1 : 0;
        break;
      case 'boolean':
        result = (a === b) ? 0 : a ? 1 : -1;
        break;
      default: // string
        result = String(a).localeCompare(String(b));
    }
    return ascending ? result : -result;
  }

  function renderTable(card, section, dataResult) {
    var cfg = section.table;
    if (!cfg || !cfg.columns) { appendError(card, 'No table config'); return; }
    if (!dataResult.ok) { appendError(card, dataResult.error); return; }
    var allData = dataResult.data;
    if (!allData || allData.length === 0) { appendError(card, 'No data'); return; }

    var totalRows = Math.min(allData.length, MAX_ROWS);
    var sourceData = allData.slice(0, totalRows); // original order preserved
    var displayData = sourceData.slice(); // mutable copy for sorting

    // Detect sort types per column (agent hint or auto-detect)
    var sortTypes = {};
    for (var c = 0; c < cfg.columns.length; c++) {
      var col = cfg.columns[c];
      sortTypes[col.field] = col.sort || detectSortType(sourceData, col.field);
    }

    // Sort state: { field, ascending } or null
    var sortState = null;

    var wrapper = document.createElement('div');
    wrapper.className = 'table-wrapper collapsed';
    card.appendChild(wrapper);

    function rebuildTable() {
      // Sort if active
      if (sortState) {
        var sf = sortState.field;
        var sa = sortState.ascending;
        var st = sortTypes[sf] || 'string';
        displayData.sort(function(a, b) {
          return compareValues(a[sf], b[sf], st, sa);
        });
      } else {
        // Restore original order
        displayData = sourceData.slice();
      }

      var html = '<table><thead><tr>';
      for (var c = 0; c < cfg.columns.length; c++) {
        var col = cfg.columns[c];
        var title = col.title || col.field;
        var style = col.width ? ' style="width:' + col.width + 'px"' : '';
        var sortClass = '';
        var indicator = ' \u2195'; // ↕ default
        if (sortState && sortState.field === col.field) {
          if (sortState.ascending) {
            sortClass = ' class="sort-asc"';
            indicator = ' \u25B2'; // ▲
          } else {
            sortClass = ' class="sort-desc"';
            indicator = ' \u25BC'; // ▼
          }
        }
        html += '<th' + sortClass + style + ' data-field="' + esc(col.field) + '">'
          + esc(title) + '<span class="sort-indicator">' + indicator + '</span></th>';
      }
      html += '</tr></thead><tbody>';
      for (var r = 0; r < displayData.length; r++) {
        html += '<tr>';
        for (var c2 = 0; c2 < cfg.columns.length; c2++) {
          html += '<td>' + esc(formatCell(displayData[r][cfg.columns[c2].field])) + '</td>';
        }
        html += '</tr>';
      }
      html += '</tbody></table>';
      wrapper.innerHTML = html;

      // Attach click handlers to headers
      var ths = wrapper.querySelectorAll('th[data-field]');
      for (var t = 0; t < ths.length; t++) {
        ths[t].addEventListener('click', onHeaderClick);
      }
    }

    function onHeaderClick(e) {
      var th = e.currentTarget;
      var field = th.getAttribute('data-field');
      if (!field) return;

      if (!sortState || sortState.field !== field) {
        // New column: ascending
        sortState = { field: field, ascending: true };
      } else if (sortState.ascending) {
        // Same column, was ascending: switch to descending
        sortState.ascending = false;
      } else {
        // Was descending: clear sort
        sortState = null;
      }
      rebuildTable();
    }

    rebuildTable();

    if (totalRows > INITIAL_ROWS) {
      var showAllText = 'Show all ' + totalRows + ' rows';
      if (allData.length > MAX_ROWS) {
        showAllText += ' (of ' + allData.length + ' total)';
      }
      addCollapseToggle(card, wrapper, showAllText);
    }
  }

  // --- Stats ---
  function renderStats(card, section, dataResult) {
    if (section.stats && section.stats.items) {
      if (!dataResult.ok) { appendError(card, dataResult.error); return; }
      renderAggregateStats(card, section.stats.items, dataResult.data || []);
      return;
    }
    if (section.inline_data) {
      renderInlineStats(card, section.inline_data);
      return;
    }
    appendError(card, 'No stats config');
  }

  function renderInlineStats(card, rows) {
    var grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (var i = 0; i < rows.length; i++) {
      grid.appendChild(statCard(formatCell(rows[i].label), formatCell(rows[i].value)));
    }
    card.appendChild(grid);
  }

  function renderAggregateStats(card, items, data) {
    var grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (var i = 0; i < items.length; i++) {
      var value = computeAggregate(items[i], data);
      grid.appendChild(statCard(items[i].label, value));
    }
    card.appendChild(grid);
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

    if (agg === 'distinct') {
      return formatCount(countDistinct(filtered, field));
    }

    var nums = [];
    for (var j = 0; j < filtered.length; j++) {
      var n = filtered[j][field];
      if (typeof n === 'number' && isFinite(n)) nums.push(n);
    }
    if (nums.length === 0) return '\u2014';

    if (agg === 'sum') {
      var sum = 0;
      for (var s = 0; s < nums.length; s++) sum += nums[s];
      return formatDecimal(sum);
    }
    if (agg === 'avg') {
      var total = 0;
      for (var a = 0; a < nums.length; a++) total += nums[a];
      return formatDecimal(total / nums.length);
    }
    if (agg === 'min') {
      var min = nums[0];
      for (var m = 1; m < nums.length; m++) if (nums[m] < min) min = nums[m];
      return formatDecimal(min);
    }
    if (agg === 'max') {
      var max = nums[0];
      for (var x = 1; x < nums.length; x++) if (nums[x] > max) max = nums[x];
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
    var card = renderSection(section, i);

    // Auto-span: tables and sections with many-category horizontal bars span full width
    var shouldSpan = false;
    if (section.type === 'table') shouldSpan = true;
    if (section.type === 'chart' && section.chart && isHorizontalBar(section.chart)) {
      var dr = getDataResult(section);
      if (dr.ok && dr.data) {
        var yField = (section.chart.encoding.y || {}).field;
        if (yField && countDistinct(dr.data, yField) > 8) shouldSpan = true;
      }
    }
    if (shouldSpan) card.classList.add('span-full');

    container.appendChild(card);
  });

})();
