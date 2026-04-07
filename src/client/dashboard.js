(function() {
  'use strict';

  // --- Bootstrap ---
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

  // ============================================================
  // FILTER STATE
  // ============================================================
  var filterState = Object.create(null);
  var filteredCache = null;
  var prevFilterCount = 0;

  function setFilter(source, field, selectedValues) {
    // selectedValues: array of values to include
    if (!selectedValues || selectedValues.length === 0) {
      // Clear this field's filter
      if (filterState[source]) {
        delete filterState[source][field];
        if (Object.keys(filterState[source]).length === 0) delete filterState[source];
      }
    } else {
      if (!filterState[source]) filterState[source] = Object.create(null);
      filterState[source][field] = Object.create(null);
      for (var i = 0; i < selectedValues.length; i++) {
        filterState[source][field][distinctKey(selectedValues[i])] = selectedValues[i];
      }
    }
    onFilterChange();
  }

  function clearFieldFilter(source, field) {
    if (filterState[source]) {
      delete filterState[source][field];
      if (Object.keys(filterState[source]).length === 0) delete filterState[source];
    }
    onFilterChange();
  }

  function clearFilters() {
    filterState = Object.create(null);
    onFilterChange();
  }

  function getActiveFilterCount() {
    var count = 0;
    for (var src in filterState) {
      for (var field in filterState[src]) {
        count += Object.keys(filterState[src][field]).length;
      }
    }
    return count;
  }

  function getFieldFilterValues(source, field) {
    if (!filterState[source] || !filterState[source][field]) return null;
    var vals = [];
    for (var key in filterState[source][field]) {
      vals.push(filterState[source][field][key]);
    }
    return vals;
  }

  function getFilteredData(source) {
    if (!filteredCache) filteredCache = Object.create(null);
    if (source in filteredCache) return filteredCache[source];

    var raw = datasets[source];
    if (!raw) return [];
    var srcFilters = filterState[source];
    if (!srcFilters) { filteredCache[source] = raw; return raw; }

    var fields = Object.keys(srcFilters);
    var result = raw.filter(function(row) {
      for (var i = 0; i < fields.length; i++) {
        var field = fields[i];
        var allowed = srcFilters[field];
        if (!(distinctKey(row[field]) in allowed)) return false;
      }
      return true;
    });

    filteredCache[source] = result;
    return result;
  }

  // ============================================================
  // SECTION REGISTRY
  // ============================================================
  var sectionRegistry = [];

  function onFilterChange() {
    filteredCache = null;
    var newCount = getActiveFilterCount();
    for (var i = 0; i < sectionRegistry.length; i++) {
      try { sectionRegistry[i].update(); }
      catch (e) { console.error('Section update error:', e); }
    }
    renderFilterBar(newCount > prevFilterCount);
    prevFilterCount = newCount;
  }

  // ============================================================
  // UTILITIES
  // ============================================================
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
    var seen = Object.create(null);
    for (var i = 0; i < data.length; i++) {
      var v = data[i][field];
      if (v !== null && v !== undefined) seen[distinctKey(v)] = true;
    }
    return Object.keys(seen).length;
  }

  function getDistinctValues(data, field) {
    var seen = Object.create(null);
    var values = [];
    for (var i = 0; i < data.length; i++) {
      var v = data[i][field];
      if (v !== null && v !== undefined) {
        var key = distinctKey(v);
        if (!(key in seen)) {
          seen[key] = true;
          values.push(v);
        }
      }
    }
    return values;
  }

  // Update a named Vega dataset using changeset
  function vegaUpdateData(view, name, rows) {
    var cs = vega.changeset().remove(function() { return true; }).insert(rows);
    view.change(name, cs).run();
  }

  // Extract a field value from a Vega SVG mark's aria-label
  // e.g. "country: US; _count: 26" → extractFieldFromLabel(label, 'country') → 'US'
  function extractFieldFromLabel(label, field) {
    var pattern = field + ': ';
    var idx = label.indexOf(pattern);
    if (idx === -1) return null;
    var start = idx + pattern.length;
    var end = label.indexOf(';', start);
    return end === -1 ? label.slice(start).trim() : label.slice(start, end).trim();
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

  // Track inline datasets
  var inlineDatasetCounter = 0;

  function getDataResult(section) {
    if (section.source) {
      if (!(section.source in datasets)) return { ok: false, error: 'Unknown dataset: ' + section.source };
      return { ok: true, data: datasets[section.source], source: section.source };
    }
    if (section.inline_data) {
      var syntheticName = section.id || ('_inline_' + inlineDatasetCounter++);
      if (!(syntheticName in datasets)) datasets[syntheticName] = section.inline_data;
      return { ok: true, data: section.inline_data, source: syntheticName };
    }
    return { ok: false, error: 'No data source configured' };
  }

  function datasetHasField(data, field) {
    for (var i = 0; i < Math.min(data.length, 10); i++) {
      if (field in data[i]) return true;
    }
    return false;
  }

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

  // ============================================================
  // COLLAPSE TOGGLE
  // ============================================================
  function addCollapseToggle(actionsEl, wrapper, initialLabel, onToggle) {
    var expanded = false;
    var collapsedLabel = initialLabel;
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
    bottomBtn.textContent = collapsedLabel;
    bottomBtn.setAttribute('aria-controls', wrapperId);
    bottomBtn.setAttribute('aria-expanded', 'false');

    function render() {
      var expStr = String(expanded);
      bottomBtn.setAttribute('aria-expanded', expStr);
      topLink.setAttribute('aria-expanded', expStr);
      if (expanded) {
        wrapper.classList.remove('collapsed');
        bottomBtn.textContent = 'Show less';
        topLink.style.display = '';
      } else {
        wrapper.classList.add('collapsed');
        bottomBtn.textContent = collapsedLabel;
        topLink.style.display = 'none';
      }
    }

    function toggle() {
      expanded = !expanded;
      render();
      if (!expanded) wrapper.parentElement.scrollIntoView({ behavior: 'smooth', block: 'start' });
      if (onToggle) onToggle(expanded);
    }

    bottomBtn.addEventListener('click', toggle);
    topLink.addEventListener('click', toggle);

    if (wrapper.nextSibling) {
      wrapper.parentNode.insertBefore(bottomBtn, wrapper.nextSibling);
    } else {
      wrapper.parentNode.appendChild(bottomBtn);
    }

    return {
      setVisible: function(visible) {
        bottomBtn.style.display = visible ? '' : 'none';
        topLink.style.display = (visible && expanded) ? '' : 'none';
        if (!visible) wrapper.classList.remove('collapsed');
        else if (!expanded) wrapper.classList.add('collapsed');
      },
      setLabel: function(label) {
        collapsedLabel = label;
        if (!expanded) bottomBtn.textContent = collapsedLabel;
      }
    };
  }

  // ============================================================
  // SECTION CARD
  // ============================================================
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

  // ============================================================
  // CHART with filter edit mode
  // ============================================================
  var CHART_COLLAPSED_HEIGHT = 350;
  var CHART_COLLAPSE_THRESHOLD = 10;
  var chartViews = Object.create(null);

  function mountChart(s, section, index, layoutMeta) {
    var cfg = section.chart;
    if (!cfg) { appendError(s.body, 'No chart config'); return null; }
    var dr = getDataResult(section);
    if (!dr.ok) { appendError(s.body, dr.error); return null; }

    var sectionKey = section.id || ('section-' + index);
    var filterField = section.interactive_filter && section.interactive_filter.field;

    if (filterField) {
      if (!dr.source) {
        appendError(s.body, 'interactive_filter requires dataset source');
        filterField = null;
      } else if (!datasetHasField(dr.data, filterField)) {
        appendError(s.body, 'interactive_filter field "' + filterField + '" not found in data');
        filterField = null;
      }
    }

    var rawData = dr.data || [];
    var data = (dr.source ? getFilteredData(dr.source) : rawData) || [];
    var mark = normalizeMark(cfg.mark);
    var markType = getMarkType(cfg.mark);

    var useStepHeight = isHorizontalBar(cfg);
    var height = 300;

    var wrapper = document.createElement('div');
    wrapper.className = 'chart-container';
    if (useStepHeight) wrapper.classList.add('chart-step-height');
    if (layoutMeta.shouldCollapse) {
      wrapper.classList.add('collapsed');
      wrapper.style.maxHeight = CHART_COLLAPSED_HEIGHT + 'px';
    }

    var div = document.createElement('div');
    wrapper.appendChild(div);
    s.body.appendChild(wrapper);

    var encoding = JSON.parse(JSON.stringify(cfg.encoding || {}));

    var vlSpec = {
      '$schema': 'https://vega.github.io/schema/vega-lite/v5.json',
      width: 'container',
      height: useStepHeight ? { step: 22 } : height,
      mark: mark,
      data: { name: 'source', values: data },
      encoding: encoding
    };

    if (markType === 'arc') {
      delete vlSpec.width;
      vlSpec.height = 300;
      vlSpec.view = { stroke: null };
    }

    var collapseCtrl = null;
    if (layoutMeta.shouldCollapse) {
      collapseCtrl = addCollapseToggle(s.actions, wrapper,
        'Show all ' + layoutMeta.categories + ' categories',
        function(exp) { wrapper.style.maxHeight = exp ? '' : CHART_COLLAPSED_HEIGHT + 'px'; });
    }

    // --- Filter edit state ---
    var filterMode = 'view'; // 'view' or 'edit'
    var pendingSelection = null; // { distinctKey: true/false }
    var filterBtn = null;
    var editControls = null;

    if (filterField && dr.source) {
      // Filter button in header
      filterBtn = document.createElement('button');
      filterBtn.className = 'filter-edit-btn';
      filterBtn.textContent = '\uD83D\uDD0D'; // 🔍
      filterBtn.title = 'Edit filter';
      filterBtn.addEventListener('click', enterEditMode);
      s.actions.insertBefore(filterBtn, s.actions.firstChild);

      // Edit controls (hidden initially)
      editControls = document.createElement('div');
      editControls.className = 'filter-edit-controls';
      editControls.style.display = 'none';

      var selectAllBtn = document.createElement('button');
      selectAllBtn.className = 'filter-edit-cancel';
      selectAllBtn.textContent = 'All';
      selectAllBtn.title = 'Select all';
      selectAllBtn.addEventListener('click', function() { selectAll(true); });

      var selectNoneBtn = document.createElement('button');
      selectNoneBtn.className = 'filter-edit-cancel';
      selectNoneBtn.textContent = 'None';
      selectNoneBtn.title = 'Deselect all';
      selectNoneBtn.addEventListener('click', function() { selectAll(false); });

      var cancelBtn = document.createElement('button');
      cancelBtn.className = 'filter-edit-cancel';
      cancelBtn.textContent = 'Cancel';
      cancelBtn.addEventListener('click', cancelEdit);

      var applyBtn = document.createElement('button');
      applyBtn.className = 'filter-edit-apply';
      applyBtn.textContent = 'Apply \u2713';
      applyBtn.addEventListener('click', applyEdit);

      editControls.appendChild(selectAllBtn);
      editControls.appendChild(selectNoneBtn);
      editControls.appendChild(cancelBtn);
      editControls.appendChild(applyBtn);
      s.actions.insertBefore(editControls, s.actions.firstChild);
    }

    function updateFilterBtnState() {
      if (!filterBtn) return;
      var hasFilter = filterState[dr.source] && filterState[dr.source][filterField];
      filterBtn.classList.toggle('filter-active', !!hasFilter);
    }

    function enterEditMode() {
      filterMode = 'edit';
      filterBtn.style.display = 'none';
      editControls.style.display = '';

      // Initialize pending selection from current filter or all-selected
      pendingSelection = Object.create(null);
      var allValues = getDistinctValues(rawData, filterField);
      var currentFilter = getFieldFilterValues(dr.source, filterField);

      for (var i = 0; i < allValues.length; i++) {
        var key = distinctKey(allValues[i]);
        if (currentFilter) {
          // Match against current filter
          pendingSelection[key] = false;
          for (var j = 0; j < currentFilter.length; j++) {
            if (distinctKey(currentFilter[j]) === key) {
              pendingSelection[key] = true;
              break;
            }
          }
        } else {
          pendingSelection[key] = true; // All selected by default
        }
      }

      // Show ALL data (unfiltered) so user can select/deselect any value
      var view = chartViews[sectionKey];
      if (view) {
        vegaUpdateData(view, 'source', rawData);
      }
      // Wait for browser to paint updated SVG before applying DOM opacity
      requestAnimationFrame(function() {
        renderChartWithSelection();
      });
    }

    function selectAll(selected) {
      if (!pendingSelection) return;
      for (var k in pendingSelection) pendingSelection[k] = selected;
      renderChartWithSelection();
    }

    function cancelEdit() {
      filterMode = 'view';
      pendingSelection = null;
      filterBtn.style.display = '';
      editControls.style.display = 'none';

      // Restore filtered data and clear opacity overrides
      var view = chartViews[sectionKey];
      if (view) {
        var filtered = dr.source ? getFilteredData(dr.source) : rawData;
        vegaUpdateData(view, 'source', filtered);
      }
      resetMarkOpacity();
    }

    function applyEdit() {
      filterMode = 'view';
      filterBtn.style.display = '';
      editControls.style.display = 'none';

      // Collect selected values
      var selected = [];
      var allValues = getDistinctValues(rawData, filterField);
      var allSelected = true;

      for (var i = 0; i < allValues.length; i++) {
        var key = distinctKey(allValues[i]);
        if (pendingSelection[key] !== false) {
          selected.push(allValues[i]);
        } else {
          allSelected = false;
        }
      }

      pendingSelection = null;

      if (allSelected) {
        // All selected = clear filter
        clearFieldFilter(dr.source, filterField);
      } else {
        setFilter(dr.source, filterField, selected);
      }
    }

    function resetMarkOpacity() {
      var svg = div.querySelector('svg');
      if (!svg) return;
      var marks = svg.querySelectorAll('.mark-rect path, .mark-arc path');
      for (var i = 0; i < marks.length; i++) {
        marks[i].style.opacity = '';
      }
    }

    // Apply opacity directly to SVG mark elements based on selection state
    function renderChartWithSelection() {
      if (!pendingSelection) return;
      var svg = div.querySelector('svg');
      if (!svg) {
        setTimeout(renderChartWithSelection, 100);
        return;
      }

      // Find all mark path/rect elements with aria-label containing filterField values
      var marks = svg.querySelectorAll('.mark-rect path, .mark-arc path');
      for (var i = 0; i < marks.length; i++) {
        var label = marks[i].getAttribute('aria-label') || '';
        var value = extractFieldFromLabel(label, filterField);
        if (value !== null) {
          var key = distinctKey(value);
          var selected = pendingSelection[key] !== false;
          marks[i].style.opacity = selected ? '1' : '0.2';
        }
      }
    }

    // Wire up Vega embed
    vegaEmbed(div, vlSpec, { actions: false, renderer: 'svg' })
      .then(function(result) {
        chartViews[sectionKey] = result.view;

        if (filterField && dr.source) {
          // Click handler: toggle selection in edit mode, enter edit mode from view
          result.view.addEventListener('click', function(event, item) {
            if (!item || !item.datum) return;
            if (!Object.prototype.hasOwnProperty.call(item.datum, filterField)) return;
            var value = item.datum[filterField];
            if (value == null) return;

            if (filterMode === 'edit') {
              // Toggle this value's selection
              var key = distinctKey(value);
              pendingSelection[key] = !pendingSelection[key];
              renderChartWithSelection();
            } else {
              // Enter edit mode and deselect everything except clicked
              enterEditMode();
              // Deselect all, then select only clicked
              for (var k in pendingSelection) pendingSelection[k] = false;
              pendingSelection[distinctKey(value)] = true;
              renderChartWithSelection();
            }
          });
        }

        // Catch up if filters changed during async embed
        var latest = dr.source ? getFilteredData(dr.source) : rawData;
        if (latest !== data) {
          vegaUpdateData(result.view, 'source', latest || []);
        }
        updateFilterBtnState();
      })
      .catch(function(err) {
        console.error('Chart "' + section.title + '":', err);
        appendError(div, 'Chart error: ' + err.message);
      });

    return function updateChart() {
      if (filterMode === 'edit') return; // Don't update while editing

      var filtered = dr.source ? getFilteredData(dr.source) : rawData;
      if (!filtered) filtered = [];

      if (collapseCtrl && isHorizontalBar(cfg)) {
        var yField = (cfg.encoding.y || {}).field;
        if (yField) {
          var cats = countDistinct(filtered, yField);
          collapseCtrl.setLabel('Show all ' + cats + ' categories');
          collapseCtrl.setVisible(cats > CHART_COLLAPSE_THRESHOLD);
        }
      }

      var view = chartViews[sectionKey];
      if (view) vegaUpdateData(view, 'source', filtered);
      updateFilterBtnState();
    };
  }

  // ============================================================
  // TABLE
  // ============================================================
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
        if (!isNaN(at) && !isNaN(bt)) result = at - bt;
        else result = String(a).localeCompare(String(b));
        break;
      case 'boolean':
        result = (a === b) ? 0 : a ? 1 : -1;
        break;
      default:
        result = String(a).localeCompare(String(b));
    }
    return ascending ? result : -result;
  }

  function mountTable(s, section) {
    var cfg = section.table;
    if (!cfg || !cfg.columns) { appendError(s.body, 'No table config'); return null; }
    var dr = getDataResult(section);
    if (!dr.ok) { appendError(s.body, dr.error); return null; }

    var sortTypes = Object.create(null);
    for (var c = 0; c < cfg.columns.length; c++) {
      var col = cfg.columns[c];
      sortTypes[col.field] = col.sort || detectSortType(dr.data, col.field);
    }

    var sortState = null;
    var wrapper = document.createElement('div');
    wrapper.className = 'table-wrapper';
    var table = document.createElement('table');
    var thead = document.createElement('thead');
    var tbody = document.createElement('tbody');
    table.appendChild(thead);
    table.appendChild(tbody);
    wrapper.appendChild(table);
    s.body.appendChild(wrapper);

    var collapseCtrl = null;

    function buildThead() {
      var tr = document.createElement('tr');
      for (var c = 0; c < cfg.columns.length; c++) {
        var col = cfg.columns[c];
        var th = document.createElement('th');
        th.setAttribute('scope', 'col');
        var isActive = sortState && sortState.field === col.field;
        th.setAttribute('aria-sort', isActive ? (sortState.ascending ? 'ascending' : 'descending') : 'none');
        if (isActive) th.className = sortState.ascending ? 'sort-asc' : 'sort-desc';
        if (col.width) th.style.width = col.width + 'px';
        var btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'table-sort-button';
        btn.setAttribute('data-field', col.field);
        btn.textContent = col.title || col.field;
        var indicator = document.createElement('span');
        indicator.className = 'sort-indicator';
        if (isActive) indicator.textContent = sortState.ascending ? ' \u25B2' : ' \u25BC';
        btn.appendChild(indicator);
        btn.addEventListener('click', onHeaderClick);
        th.appendChild(btn);
        tr.appendChild(th);
      }
      thead.innerHTML = '';
      thead.appendChild(tr);
    }

    function rebuildTbody() {
      var allData = dr.source ? getFilteredData(dr.source) : dr.data;
      if (!allData) allData = [];
      var totalRows = Math.min(allData.length, MAX_ROWS);
      var sourceData = allData.slice(0, totalRows);
      var displayData = sourceData.slice();

      if (sortState) {
        var sf = sortState.field, sa = sortState.ascending, st = sortTypes[sf] || 'string';
        displayData.sort(function(a, b) { return compareValues(a[sf], b[sf], st, sa); });
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

      var needsCollapse = totalRows > INITIAL_ROWS;
      if (needsCollapse && !collapseCtrl) {
        wrapper.classList.add('collapsed');
        var label = 'Show all ' + totalRows + ' rows';
        if (allData.length > MAX_ROWS) label += ' (of ' + allData.length + ' total)';
        collapseCtrl = addCollapseToggle(s.actions, wrapper, label);
      } else if (collapseCtrl) {
        var lbl = 'Show all ' + totalRows + ' rows';
        if (allData.length > MAX_ROWS) lbl += ' (of ' + allData.length + ' total)';
        collapseCtrl.setLabel(lbl);
        collapseCtrl.setVisible(needsCollapse);
      }
    }

    function onHeaderClick(e) {
      var field = e.currentTarget.getAttribute('data-field');
      if (!field) return;
      if (!sortState || sortState.field !== field) sortState = { field: field, ascending: true };
      else if (sortState.ascending) sortState.ascending = false;
      else sortState = null;
      buildThead();
      rebuildTbody();
    }

    buildThead();
    rebuildTbody();

    return function updateTable() { rebuildTbody(); };
  }

  // ============================================================
  // STATS
  // ============================================================
  function mountStats(s, section) {
    var dr = getDataResult(section);

    if (section.stats && section.stats.items) {
      if (!dr.ok) { appendError(s.body, dr.error); return null; }
      var grid = document.createElement('div');
      grid.className = 'stats-grid';
      s.body.appendChild(grid);
      var items = section.stats.items;

      function rebuild() {
        var data = dr.source ? getFilteredData(dr.source) : dr.data || [];
        grid.innerHTML = '';
        for (var i = 0; i < items.length; i++) {
          grid.appendChild(statCard(items[i].label, computeAggregate(items[i], data)));
        }
      }
      rebuild();
      return function updateStats() { rebuild(); };
    }

    if (section.inline_data) {
      renderInlineStats(s.body, section.inline_data);
      return null;
    }

    appendError(s.body, 'No stats config');
    return null;
  }

  function renderInlineStats(parent, rows) {
    var grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (var i = 0; i < rows.length; i++) {
      grid.appendChild(statCard(formatCell(rows[i].label), formatCell(rows[i].value)));
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
    var agg = item.aggregate, field = item.field;
    if (agg === 'count') return formatCount(filtered.length);
    if (!field) return '\u26a0 missing field';
    if (agg === 'distinct') return formatCount(countDistinct(filtered, field));
    var nums = [];
    for (var j = 0; j < filtered.length; j++) {
      var n = filtered[j][field];
      if (typeof n === 'number' && isFinite(n)) nums.push(n);
    }
    if (nums.length === 0) return '\u2014';
    if (agg === 'sum') { var sum = 0; for (var si = 0; si < nums.length; si++) sum += nums[si]; return formatDecimal(sum); }
    if (agg === 'avg') { var tot = 0; for (var ai = 0; ai < nums.length; ai++) tot += nums[ai]; return formatDecimal(tot / nums.length); }
    if (agg === 'min') { var min = nums[0]; for (var mi = 1; mi < nums.length; mi++) if (nums[mi] < min) min = nums[mi]; return formatDecimal(min); }
    if (agg === 'max') { var max = nums[0]; for (var xi = 1; xi < nums.length; xi++) if (nums[xi] > max) max = nums[xi]; return formatDecimal(max); }
    return '\u26a0 unknown: ' + agg;
  }

  function valueEquals(a, b) {
    if (a === b) return true;
    if (a == null || b == null) return a === b;
    return false;
  }

  // ============================================================
  // FILTER BAR
  // ============================================================
  var filterBarEl = null;

  function createFilterBar() {
    if (!container.parentNode) return;
    filterBarEl = document.createElement('div');
    filterBarEl.className = 'filter-bar';
    filterBarEl.style.display = 'none';
    container.parentNode.insertBefore(filterBarEl, container);
  }

  function renderFilterBar(shouldPulse) {
    if (!filterBarEl) return;
    var count = getActiveFilterCount();
    if (count === 0) {
      filterBarEl.style.display = 'none';
      filterBarEl.classList.remove('filter-bar-pulse');
      return;
    }

    filterBarEl.style.display = '';
    filterBarEl.innerHTML = '';

    var label = document.createElement('span');
    label.className = 'filter-bar-label';
    label.textContent = 'Filters:';
    filterBarEl.appendChild(label);

    var tags = document.createElement('div');
    tags.className = 'filter-bar-tags';

    for (var src in filterState) {
      for (var field in filterState[src]) {
        var values = filterState[src][field];
        var names = [];
        for (var key in values) names.push(formatCell(values[key]));
        var tag = document.createElement('button');
        tag.className = 'filter-tag';
        tag.textContent = field + ': ' + names.join(', ');
        tag.setAttribute('title', 'Remove filter');
        (function(s, f) {
          tag.addEventListener('click', function() { clearFieldFilter(s, f); });
        })(src, field);
        tags.appendChild(tag);
      }
    }
    filterBarEl.appendChild(tags);

    var resetBtn = document.createElement('button');
    resetBtn.className = 'filter-bar-reset';
    resetBtn.textContent = 'Reset all';
    resetBtn.addEventListener('click', clearFilters);
    filterBarEl.appendChild(resetBtn);

    filterBarEl.classList.remove('filter-bar-pulse');
    if (shouldPulse) {
      void filterBarEl.offsetWidth;
      filterBarEl.classList.add('filter-bar-pulse');
    }
  }

  // ============================================================
  // RENDER ALL SECTIONS
  // ============================================================
  createFilterBar();

  spec.sections.forEach(function(section, i) {
    var dataResult = getDataResult(section);
    var layoutMeta = getSectionLayoutMeta(section, dataResult);
    var s = createSectionCard(section, i);

    var updateFn = null;
    try {
      switch (section.type) {
        case 'chart': updateFn = mountChart(s, section, i, layoutMeta); break;
        case 'table': updateFn = mountTable(s, section); break;
        case 'stats': updateFn = mountStats(s, section); break;
        case 'list': appendError(s.body, 'List rendering coming soon'); break;
        default: appendError(s.body, 'Unknown section type: ' + String(section.type));
      }
    } catch (e) {
      appendError(s.body, 'Render error: ' + e.message);
      console.error('Section "' + section.title + '":', e);
    }

    if (updateFn) sectionRegistry.push({ section: section, update: updateFn });
    if (layoutMeta.spanFull) s.card.classList.add('span-full');
    container.appendChild(s.card);
  });

})();
