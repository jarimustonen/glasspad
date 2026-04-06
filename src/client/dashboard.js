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
  function renderChart(card, section, dataResult, index) {
    var cfg = section.chart;
    if (!cfg) { appendError(card, 'No chart config'); return; }

    var sectionKey = section.id || ('section-' + index);
    var div = document.createElement('div');
    div.className = 'chart-container';
    card.appendChild(div);

    var data = dataResult.ok ? dataResult.data : [];
    var mark = cfg.mark === 'arc' ? { type: 'arc', tooltip: true } : cfg.mark;

    var vlSpec = {
      '$schema': 'https://vega.github.io/schema/vega-lite/v5.json',
      width: 'container',
      height: 300,
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
  }

  // --- Table ---
  var DEFAULT_MAX_ROWS = 1000;

  function renderTable(card, section, dataResult) {
    var cfg = section.table;
    if (!cfg || !cfg.columns) { appendError(card, 'No table config'); return; }
    if (!dataResult.ok) { appendError(card, dataResult.error); return; }
    var data = dataResult.data;
    if (!data || data.length === 0) { appendError(card, 'No data'); return; }

    var maxRows = DEFAULT_MAX_ROWS;
    var truncated = data.length > maxRows;
    var rows = truncated ? data.slice(0, maxRows) : data;

    // Build HTML string once (not innerHTML +=)
    var html = '<table><thead><tr>';
    for (var c = 0; c < cfg.columns.length; c++) {
      var col = cfg.columns[c];
      var title = col.title || col.field;
      var style = col.width ? ' style="width:' + col.width + 'px"' : '';
      html += '<th' + style + '>' + esc(title) + '</th>';
    }
    html += '</tr></thead><tbody>';

    for (var r = 0; r < rows.length; r++) {
      html += '<tr>';
      for (var c2 = 0; c2 < cfg.columns.length; c2++) {
        html += '<td>' + esc(formatCell(rows[r][cfg.columns[c2].field])) + '</td>';
      }
      html += '</tr>';
    }
    html += '</tbody></table>';

    var wrapper = document.createElement('div');
    wrapper.innerHTML = html;
    card.appendChild(wrapper);

    if (truncated) {
      var msg = document.createElement('p');
      msg.className = 'table-truncated';
      msg.textContent = 'Showing ' + maxRows + ' of ' + data.length + ' rows';
      card.appendChild(msg);
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
      var sc = document.createElement('div');
      sc.className = 'stat-card';
      var val = document.createElement('div');
      val.className = 'stat-value';
      val.textContent = formatCell(rows[i].value);
      var lbl = document.createElement('div');
      lbl.className = 'stat-label';
      lbl.textContent = formatCell(rows[i].label);
      sc.appendChild(val);
      sc.appendChild(lbl);
      grid.appendChild(sc);
    }
    card.appendChild(grid);
  }

  function renderAggregateStats(card, items, data) {
    var grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (var i = 0; i < items.length; i++) {
      var value = computeAggregate(items[i], data);
      var sc = document.createElement('div');
      sc.className = 'stat-card';
      var val = document.createElement('div');
      val.className = 'stat-value';
      val.textContent = value;
      var lbl = document.createElement('div');
      lbl.className = 'stat-label';
      lbl.textContent = items[i].label;
      sc.appendChild(val);
      sc.appendChild(lbl);
      grid.appendChild(sc);
    }
    card.appendChild(grid);
  }

  function computeAggregate(item, data) {
    var filtered = data;
    var whereClause = item.where || item.where_clause;
    if (whereClause) {
      var keys = Object.keys(whereClause);
      filtered = data.filter(function(row) {
        for (var i = 0; i < keys.length; i++) {
          var k = keys[i];
          if (!valueEquals(row[k], whereClause[k])) return false;
        }
        return true;
      });
    }

    var agg = item.aggregate;
    var field = item.field;

    if (agg === 'count') return formatCount(filtered.length);
    if (!field) return '\u26a0 missing field';

    if (agg === 'distinct') {
      var seen = {};
      for (var i = 0; i < filtered.length; i++) {
        var v = filtered[i][field];
        if (v !== null && v !== undefined) {
          seen[distinctKey(v)] = true;
        }
      }
      return formatCount(Object.keys(seen).length);
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

  function distinctKey(v) {
    if (v === null) return 'null';
    return typeof v + ':' + String(v);
  }

  // --- Render all sections ---
  spec.sections.forEach(function(section, i) {
    container.appendChild(renderSection(section, i));
  });

})();
