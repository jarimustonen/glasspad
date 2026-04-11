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

  // Timezone-aware hour extraction (spec.timezone: "utc" | "local" | null)
  var useUtc = spec.timezone === 'utc';
  function getHourOfDate(d) {
    return useUtc ? d.getUTCHours() : d.getHours();
  }

  // Extract a temporal unit value from a Date for the given timeUnit
  // Vega-Lite: 'day' = day-of-week (0=Sun..6=Sat), 'date' = day-of-month (1-31)
  function extractTimeUnit(d, timeUnit) {
    switch (timeUnit) {
      case 'hours': case 'utchours': return getHourOfDate(d);
      case 'day': case 'utcday':
        return useUtc ? d.getUTCDay() : d.getDay(); // 0=Sun, 6=Sat
      case 'date': case 'utcdate':
        return useUtc ? d.getUTCDate() : d.getDate(); // 1-31
      case 'month': case 'utcmonth':
        return (useUtc ? d.getUTCMonth() : d.getMonth()) + 1; // 1-12
      case 'year': case 'utcyear':
        return useUtc ? d.getUTCFullYear() : d.getFullYear();
      case 'yearmonthdate': case 'utcyearmonthdate':
        // Truncate to midnight and return as ms timestamp (unique per day)
        if (useUtc) return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
        return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
      default: return getHourOfDate(d);
    }
  }

  var DAY_NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

  // Format a time unit value for display
  function formatTimeUnitValue(val, timeUnit) {
    switch (timeUnit) {
      case 'hours': case 'utchours':
        return (val < 10 ? '0' : '') + val + ':00';
      case 'day': case 'utcday':
        return DAY_NAMES[val] || String(val);
      case 'date': case 'utcdate':
        return String(val); // day-of-month number
      case 'month': case 'utcmonth':
        var months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
        return months[val - 1] || String(val);
      case 'year': case 'utcyear':
        return String(val);
      case 'yearmonthdate': case 'utcyearmonthdate':
        // val is ms timestamp truncated to midnight
        var d = new Date(val);
        return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
      default: return String(val);
    }
  }

  // ============================================================
  // FILTER STATE
  // ============================================================
  var filterState = Object.create(null);       // discrete: filterState[source][field] = { distinctKey: value }
  var rangeFilterState = Object.create(null);  // temporal: rangeFilterState[source][field] = { min: ms, max: ms }
  var hourFilterState = Object.create(null);   // hour-of-day: hourFilterState[source][field] = { min: 0-23, max: 0-23 }
  var filteredCache = null;
  var prevFilterCount = 0;

  function setFilter(source, field, selectedValues) {
    // selectedValues: null = clear filter, [] = empty set (show nothing), [...] = include these
    if (selectedValues == null) {
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

  function setRangeFilter(source, field, min, max) {
    if (!rangeFilterState[source]) rangeFilterState[source] = Object.create(null);
    rangeFilterState[source][field] = { min: min, max: max };
    onFilterChange();
  }

  function clearRangeFilter(source, field) {
    if (rangeFilterState[source]) {
      delete rangeFilterState[source][field];
      if (Object.keys(rangeFilterState[source]).length === 0) delete rangeFilterState[source];
    }
    onFilterChange();
  }

  function setHourFilter(source, field, minH, maxH, timeUnit) {
    if (!hourFilterState[source]) hourFilterState[source] = Object.create(null);
    hourFilterState[source][field] = { min: minH, max: maxH, timeUnit: timeUnit || 'hours' };
    onFilterChange();
  }

  function clearHourFilter(source, field) {
    if (hourFilterState[source]) {
      delete hourFilterState[source][field];
      if (Object.keys(hourFilterState[source]).length === 0) delete hourFilterState[source];
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
    rangeFilterState = Object.create(null);
    hourFilterState = Object.create(null);
    // Clear any Vega brush selection visuals
    for (var key in chartViews) {
      try { chartViews[key].signal('brush', {}).run(); } catch (e) { /* no brush */ }
    }
    onFilterChange();
  }

  function getActiveFilterCount() {
    var count = 0;
    for (var src in filterState) {
      for (var field in filterState[src]) {
        count += Object.keys(filterState[src][field]).length;
      }
    }
    for (var src2 in rangeFilterState) {
      count += Object.keys(rangeFilterState[src2]).length;
    }
    for (var src3 in hourFilterState) {
      count += Object.keys(hourFilterState[src3]).length;
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
    var srcRanges = rangeFilterState[source];
    var srcHours = hourFilterState[source];
    if (!srcFilters && !srcRanges && !srcHours) { filteredCache[source] = raw; return raw; }

    var fields = srcFilters ? Object.keys(srcFilters) : [];
    var rangeFields = srcRanges ? Object.keys(srcRanges) : [];
    var hourFields = srcHours ? Object.keys(srcHours) : [];
    var result = raw.filter(function(row) {
      for (var i = 0; i < fields.length; i++) {
        var field = fields[i];
        var allowed = srcFilters[field];
        if (!(distinctKey(row[field]) in allowed)) return false;
      }
      for (var j = 0; j < rangeFields.length; j++) {
        var rf = rangeFields[j];
        var range = srcRanges[rf];
        var v = row[rf];
        if (v == null) return false;
        var t = (typeof v === 'number') ? v : Date.parse(v);
        if (isNaN(t) || t < range.min || t > range.max) return false;
      }
      for (var h = 0; h < hourFields.length; h++) {
        var hf = hourFields[h];
        var hRange = srcHours[hf];
        var hv = row[hf];
        if (hv == null) return false;
        var d = new Date(hv);
        if (!isFinite(d.getTime())) return false;
        var unitVal = extractTimeUnit(d, hRange.timeUnit || 'hours');
        if (!isFinite(unitVal) || unitVal < hRange.min || unitVal > hRange.max) return false;
      }
      return true;
    });

    filteredCache[source] = result;
    return result;
  }

  // Cached version of getFilteredDataExcluding (invalidated with filteredCache)
  var excludeCache = null;

  function getFilteredDataExcluding(source, excludeField, excludeKind) {
    if (!excludeCache) excludeCache = Object.create(null);
    var cacheKey = source + '|' + excludeField + '|' + excludeKind;
    if (cacheKey in excludeCache) return excludeCache[cacheKey];

    var raw = datasets[source];
    if (!raw) return [];
    var srcFilters = filterState[source];
    var srcRanges = rangeFilterState[source];
    var srcHours = hourFilterState[source];

    var fields = [];
    if (srcFilters) {
      for (var f in srcFilters) {
        if (!(excludeKind === 'discrete' && f === excludeField)) fields.push(f);
      }
    }
    var rangeFields = [];
    if (srcRanges) {
      for (var rf in srcRanges) {
        if (!(excludeKind === 'range' && rf === excludeField)) rangeFields.push(rf);
      }
    }
    var hourFields = [];
    if (srcHours) {
      for (var hf in srcHours) {
        if (!(excludeKind === 'hour' && hf === excludeField)) hourFields.push(hf);
      }
    }

    if (fields.length === 0 && rangeFields.length === 0 && hourFields.length === 0) return raw;

    var result = raw.filter(function(row) {
      for (var i = 0; i < fields.length; i++) {
        var allowed = srcFilters[fields[i]];
        if (!(distinctKey(row[fields[i]]) in allowed)) return false;
      }
      for (var j = 0; j < rangeFields.length; j++) {
        var range = srcRanges[rangeFields[j]];
        var v = row[rangeFields[j]];
        if (v == null) return false;
        var t = (typeof v === 'number') ? v : Date.parse(v);
        if (isNaN(t) || t < range.min || t > range.max) return false;
      }
      for (var h = 0; h < hourFields.length; h++) {
        var hRange = srcHours[hourFields[h]];
        var hv = row[hourFields[h]];
        if (hv == null) return false;
        var d = new Date(hv);
        if (!isFinite(d.getTime())) return false;
        var unitVal = extractTimeUnit(d, hRange.timeUnit || 'hours');
        if (!isFinite(unitVal) || unitVal < hRange.min || unitVal > hRange.max) return false;
      }
      return true;
    });
    excludeCache[cacheKey] = result;
    return result;
  }

  // ============================================================
  // SECTION REGISTRY
  // ============================================================
  var sectionRegistry = [];

  function onFilterChange() {
    filteredCache = null;
    excludeCache = null;
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

  // Infer the JS type of a field from data (first non-null value)
  function inferFieldType(data, field) {
    for (var i = 0; i < data.length; i++) {
      var v = data[i][field];
      if (v !== null && v !== undefined) return typeof v;
    }
    return 'string';
  }

  // Coerce a string extracted from aria-label back to the field's native type
  function coerceExtractedValue(strVal, fieldType) {
    if (fieldType === 'number') { var n = Number(strVal); return isNaN(n) ? strVal : n; }
    if (fieldType === 'boolean') return strVal === 'true';
    return strVal;
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

  function formatTemporalRange(minMs, maxMs) {
    var d1 = new Date(minMs), d2 = new Date(maxMs);
    var tzOpt = useUtc ? { timeZone: 'UTC' } : {};
    var timeFmt = Object.assign({ hour: '2-digit', minute: '2-digit' }, tzOpt);
    var sameDay = d1.toLocaleDateString(undefined, tzOpt) === d2.toLocaleDateString(undefined, tzOpt);
    if (sameDay) {
      return d1.toLocaleDateString(undefined, tzOpt) + ' ' +
        d1.toLocaleTimeString(undefined, timeFmt) + ' \u2013 ' +
        d2.toLocaleTimeString(undefined, timeFmt);
    }
    var dateFmt = Object.assign({ month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }, tzOpt);
    return d1.toLocaleDateString(undefined, dateFmt) + ' \u2013 ' +
      d2.toLocaleDateString(undefined, dateFmt);
  }

  // Compute [min, max] ISO strings for a temporal field
  function temporalExtent(data, field) {
    var min = Infinity, max = -Infinity;
    for (var i = 0; i < data.length; i++) {
      var v = data[i][field];
      if (v == null) continue;
      var t = (typeof v === 'number') ? v : Date.parse(v);
      if (isNaN(t)) continue;
      if (t < min) min = t;
      if (t > max) max = t;
    }
    if (min === Infinity) return null;
    return [new Date(min).toISOString(), new Date(max).toISOString()];
  }

  // Update a named Vega dataset using changeset
  function vegaUpdateData(view, name, rows) {
    var cs = vega.changeset().remove(function() { return true; }).insert(rows);
    view.change(name, cs).run();
  }

  // Extract a field value from a Vega SVG mark's aria-label
  // e.g. "country: US; _count: 26" → extractFieldFromLabel(label, 'country') → 'US'
  function extractFieldFromLabel(label, field) {
    // Try exact match first: "field: value"
    // Then timeUnit match: "field (timeUnit): value"
    var patterns = [field + ': ', field + ' ('];
    for (var pi = 0; pi < patterns.length; pi++) {
      var pattern = patterns[pi];
      var idx = label.indexOf(pattern);
      while (idx !== -1) {
        if (idx === 0 || label.substring(idx - 2, idx) === '; ') {
          // For timeUnit pattern, skip past the closing "): "
          var start;
          if (pi === 1) {
            var paren = label.indexOf('): ', idx);
            if (paren === -1) { idx = label.indexOf(pattern, idx + 1); continue; }
            start = paren + 3;
          } else {
            start = idx + pattern.length;
          }
          var end = label.indexOf(';', start);
          return end === -1 ? label.slice(start).trim() : label.slice(start, end).trim();
        }
        idx = label.indexOf(pattern, idx + 1);
      }
    }
    return null;
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
    for (var i = 0; i < data.length; i++) {
      if (data[i] && field in data[i]) return true;
    }
    return false;
  }

  function getSectionLayoutMeta(section, dataResult) {
    var meta = { spanFull: false, categories: 0, shouldCollapse: false };
    if (section.type === 'table' || section.type === 'list' || section.type === 'markdown') { meta.spanFull = true; return meta; }
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
      },
      isExpanded: function() { return expanded; },
      setExpanded: function(exp) {
        if (exp !== expanded) {
          expanded = exp;
          render();
          if (onToggle) onToggle(expanded);
        }
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

    // Fix axes: integer-only count ticks
    ['x', 'y'].forEach(function(ch) {
      var enc = encoding[ch];
      if (!enc) return;
      if (enc.aggregate === 'count') {
        if (!enc.axis) enc.axis = {};
        // Integer-only: hide labels AND ticks for fractional values
        enc.axis.labelExpr = "datum.value === floor(datum.value) ? format(datum.value, 'd') : ''";
        enc.axis.tickColor = { expr: "datum.value === floor(datum.value) ? '#888' : 'transparent'" };
        enc.axis.gridColor = { expr: "datum.value === floor(datum.value) ? '#ddd' : 'transparent'" };
      }
    });

    // Temporal + timeUnit: place labels at bin midpoints (not boundaries)
    // Continuous time scales ignore tickBand/bandPosition, so we compute
    // midpoint timestamps from the data and set axis.values explicitly.
    ['x', 'y'].forEach(function(ch) {
      var enc = encoding[ch];
      if (!enc) return;
      if (enc.type === 'temporal' && enc.timeUnit && dr.source) {
        var srcData = rawData.length > 0 ? rawData : data;
        // Collect unique bin-start timestamps
        var seen = {};
        var binStarts = [];
        for (var i = 0; i < srcData.length; i++) {
          var v = srcData[i][enc.field];
          if (v == null) continue;
          var ts = extractTimeUnit(new Date(v), enc.timeUnit);
          if (!seen[ts]) { seen[ts] = true; binStarts.push(ts); }
        }
        binStarts.sort(function(a, b) { return a - b; });
        // Compute bin duration and midpoints
        if (binStarts.length > 1) {
          var midpoints = [];
          var halfBin = (binStarts[1] - binStarts[0]) / 2;
          for (var j = 0; j < binStarts.length; j++) {
            midpoints.push(binStarts[j] + halfBin);
          }
          if (!enc.axis) enc.axis = {};
          enc.axis.values = midpoints;
          enc.axis.ticks = false;
        }
      }
    });

    // Lock temporal domain to unfiltered data extent (non-timeUnit only)
    if (dr.source && rawData.length > 0) {
      ['x', 'y'].forEach(function(ch) {
        var enc = encoding[ch];
        if (!enc) return;
        if (enc.type === 'temporal' && enc.field && !enc.timeUnit) {
          var extent = temporalExtent(rawData, enc.field);
          if (extent) {
            if (!enc.scale) enc.scale = {};
            enc.scale.domain = extent;
          }
        }
      });
    }

    // Detect temporal channel for brush selection
    var temporalChannel = null;
    var temporalField = null;
    var hasTimeUnit = false;
    var temporalTimeUnit = null;
    ['x', 'y'].forEach(function(ch) {
      var enc = encoding[ch];
      if (enc && enc.type === 'temporal' && enc.field && dr.source) {
        temporalChannel = ch;
        temporalField = enc.field;
        if (enc.timeUnit) {
          hasTimeUnit = true;
          temporalTimeUnit = enc.timeUnit;
        }
      }
    });

    var vlSpec = {
      '$schema': 'https://vega.github.io/schema/vega-lite/v5.json',
      width: 'container',
      height: useStepHeight ? { step: 22 } : height
    };

    if (hasTimeUnit && dr.source) {
      // Ghost layer pattern: invisible rawData layer locks both axes,
      // visible filtered layer shows actual bars
      var ghostMark = JSON.parse(JSON.stringify(mark));
      ghostMark.opacity = 0;
      ghostMark.tooltip = false;

      vlSpec.encoding = encoding;
      vlSpec.layer = [
        { data: { name: 'source_raw', values: rawData }, mark: ghostMark },
        { data: { name: 'source', values: data }, mark: mark }
      ];
    } else {
      vlSpec.mark = mark;
      vlSpec.data = { name: 'source', values: data };
      vlSpec.encoding = encoding;
    }

    // Add interval selection (brush) for non-timeUnit temporal charts only
    // (timeUnit charts like hours are cyclical — brush range filter doesn't apply)
    if (temporalChannel && !filterField && !hasTimeUnit) {
      vlSpec.params = [{
        name: 'brush',
        select: { type: 'interval', encodings: [temporalChannel] }
      }];
    }

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
    var selectionRetries = 0;
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

    // --- Temporal unit filter UI ---
    var temporalFilterBtn = null;
    var temporalEditControls = null;
    var rangeSlider = null;
    var temporalFilterMode = 'view';
    var onBarMouseDown = null;
    var onBarMouseMove = null;

    if (hasTimeUnit && temporalField && dr.source && !filterField) {
      // Collect sorted unique time unit values from raw data
      var tu = temporalTimeUnit || 'hours';
      var allUnits = Object.create(null);
      for (var hi = 0; hi < rawData.length; hi++) {
        var hv = rawData[hi][temporalField];
        if (hv != null) {
          var d = new Date(hv);
          if (isFinite(d.getTime())) allUnits[extractTimeUnit(d, tu)] = true;
        }
      }
      // unitList: sorted unique bin values; slider works on boundary indices 0..N
      // where N = unitList.length.  Boundary i sits at the left edge of bin i
      // (boundary N = right edge of last bin).  Selected bins: [minIdx, maxIdx).
      var unitList = Object.keys(allUnits).map(Number).sort(function(a, b) { return a - b; });
      var minUnitData = 0;                        // boundary index (always 0)
      var maxUnitData = unitList.length;           // boundary index (= N, right edge of last bin)

      temporalFilterBtn = document.createElement('button');
      temporalFilterBtn.className = 'filter-edit-btn';
      temporalFilterBtn.textContent = '\uD83D\uDD0D'; // 🔍
      temporalFilterBtn.title = 'Filter by time range';
      s.actions.insertBefore(temporalFilterBtn, s.actions.firstChild);

      temporalEditControls = document.createElement('div');
      temporalEditControls.className = 'filter-edit-controls';
      temporalEditControls.style.display = 'none';

      // Range slider container
      rangeSlider = document.createElement('div');
      rangeSlider.className = 'hour-range-slider';

      var sliderLabel = document.createElement('span');
      sliderLabel.className = 'hour-range-label';

      var sliderTrack = document.createElement('div');
      sliderTrack.className = 'hour-range-track';

      var sliderFill = document.createElement('div');
      sliderFill.className = 'hour-range-fill';
      sliderTrack.appendChild(sliderFill);

      var handleMin = document.createElement('div');
      handleMin.className = 'hour-range-handle';
      handleMin.setAttribute('data-which', 'min');
      handleMin.setAttribute('tabindex', '0');
      handleMin.setAttribute('role', 'slider');
      handleMin.setAttribute('aria-label', 'Minimum hour');
      sliderTrack.appendChild(handleMin);

      var handleMax = document.createElement('div');
      handleMax.className = 'hour-range-handle';
      handleMax.setAttribute('data-which', 'max');
      handleMax.setAttribute('tabindex', '0');
      handleMax.setAttribute('role', 'slider');
      handleMax.setAttribute('aria-label', 'Maximum hour');
      sliderTrack.appendChild(handleMax);

      rangeSlider.appendChild(sliderTrack);
      rangeSlider.appendChild(sliderLabel);

      var pendingMin = minUnitData;
      var pendingMax = maxUnitData;

      // Format a slider index as display text (index → actual value → text)
      function formatUnit(idx) {
        var val = unitList[idx];
        return val !== undefined ? formatTimeUnitValue(val, tu) : String(idx);
      }

      function alignSliderToChart() {
        var svg = div.querySelector('svg');
        if (!svg) return;
        // Find the plot frame background rect — its bbox gives us the plot area
        var bg = svg.querySelector('g.mark-group.role-frame > g > path.background');
        if (!bg) return;
        var bgRect = bg.getBoundingClientRect();
        var sliderParentRect = rangeSlider.getBoundingClientRect();
        // Slider track spans full plot width so stops land on bar boundaries:
        // stop 0 = left edge of first bar, stop N-1 = right edge of last bar
        var trackWidth = bgRect.width;
        var leftOffset = bgRect.left - sliderParentRect.left;
        sliderTrack.style.marginLeft = leftOffset + 'px';
        sliderTrack.style.width = trackWidth + 'px';
      }

      function updateSliderUI() {
        var range = maxUnitData - minUnitData;
        var pctMin = range > 0 ? ((pendingMin - minUnitData) / range) * 100 : 0;
        var pctMax = range > 0 ? ((pendingMax - minUnitData) / range) * 100 : 100;
        sliderFill.style.left = pctMin + '%';
        sliderFill.style.width = (pctMax - pctMin) + '%';
        handleMin.style.left = pctMin + '%';
        handleMax.style.left = pctMax + '%';
        handleMin.setAttribute('aria-valuemin', minUnitData);
        handleMin.setAttribute('aria-valuemax', pendingMax);
        handleMin.setAttribute('aria-valuenow', pendingMin);
        handleMax.setAttribute('aria-valuemin', pendingMin);
        handleMax.setAttribute('aria-valuemax', maxUnitData);
        handleMax.setAttribute('aria-valuenow', pendingMax);
        // Label shows selected bins (not boundaries):
        // [min, max) boundaries → bins min..max-1
        if (pendingMin >= pendingMax) {
          sliderLabel.textContent = '\u2013'; // empty selection
        } else if (pendingMax - pendingMin === 1) {
          sliderLabel.textContent = formatUnit(pendingMin); // single bin
        } else {
          sliderLabel.textContent = formatUnit(pendingMin) + ' \u2013 ' + formatUnit(pendingMax - 1);
        }
        // Dim bars outside selected range
        dimBarsOutsideRange(pendingMin, pendingMax);
      }

      // Dim chart bars outside the selected boundary range [minIdx, maxIdx)
      // Only targets the second (visible) layer's marks, skipping the ghost layer
      function dimBarsOutsideRange(minIdx, maxIdx) {
        var svg = div.querySelector('svg');
        if (!svg) return;
        var markGroups = svg.querySelectorAll('.mark-rect.role-mark:not([class*=brush])');
        var targetGroup = markGroups.length > 1 ? markGroups[markGroups.length - 1] : markGroups[0];
        if (!targetGroup) return;
        var marks = targetGroup.querySelectorAll('path');
        // Build a set of included bin values from boundary indices [minIdx, maxIdx)
        var includedBins = Object.create(null);
        for (var bi = minIdx; bi < maxIdx && bi < unitList.length; bi++) {
          includedBins[unitList[bi]] = true;
        }
        for (var mi = 0; mi < marks.length; mi++) {
          var label = marks[mi].getAttribute('aria-label') || '';
          var dateStr = extractFieldFromLabel(label, temporalField);
          if (dateStr) {
            var barUnit = extractTimeUnit(new Date(dateStr), tu);
            marks[mi].style.opacity = includedBins[barUnit] ? '1' : '0.15';
          } else {
            var hMatch = label.match(/(\d{1,2}):00/);
            if (hMatch) {
              var barHour = parseInt(hMatch[1], 10);
              marks[mi].style.opacity = includedBins[barHour] ? '1' : '0.15';
            }
          }
        }
      }

      function clearBarDimming() {
        var svg = div.querySelector('svg');
        if (!svg) return;
        var marks = svg.querySelectorAll('.mark-rect.role-mark path');
        for (var ci = 0; ci < marks.length; ci++) {
          marks[ci].style.opacity = '';
        }
      }

      function hourFromPct(pct) {
        var range = maxUnitData - minUnitData;
        var h = Math.round(minUnitData + (pct / 100) * range);
        return Math.max(minUnitData, Math.min(maxUnitData, h));
      }

      function startDrag(e, which) {
        e.preventDefault();
        // Bring dragged handle to front so it stays grabbable when overlapping
        handleMin.style.zIndex = (which === 'min') ? '3' : '2';
        handleMax.style.zIndex = (which === 'max') ? '3' : '2';
        var trackRect = sliderTrack.getBoundingClientRect();
        function onMove(ev) {
          var clientX = ev.touches ? ev.touches[0].clientX : ev.clientX;
          var pct = Math.max(0, Math.min(100, ((clientX - trackRect.left) / trackRect.width) * 100));
          var h = hourFromPct(pct);
          if (which === 'min') {
            pendingMin = Math.min(h, pendingMax);
          } else {
            pendingMax = Math.max(h, pendingMin);
          }
          updateSliderUI();
        }
        function onUp() {
          document.removeEventListener('mousemove', onMove);
          document.removeEventListener('mouseup', onUp);
          document.removeEventListener('touchmove', onMove);
          document.removeEventListener('touchend', onUp);
          document.removeEventListener('touchcancel', onUp);
        }
        document.addEventListener('mousemove', onMove);
        document.addEventListener('mouseup', onUp);
        document.addEventListener('touchmove', onMove);
        document.addEventListener('touchend', onUp);
        document.addEventListener('touchcancel', onUp);
      }

      handleMin.addEventListener('mousedown', function(e) { startDrag(e, 'min'); });
      handleMin.addEventListener('touchstart', function(e) { startDrag(e, 'min'); });
      handleMax.addEventListener('mousedown', function(e) { startDrag(e, 'max'); });
      handleMax.addEventListener('touchstart', function(e) { startDrag(e, 'max'); });

      // Keyboard accessibility for slider handles
      function handleSliderKey(e, which) {
        var delta = (e.key === 'ArrowRight' || e.key === 'ArrowUp') ? 1
          : (e.key === 'ArrowLeft' || e.key === 'ArrowDown') ? -1 : 0;
        if (!delta) return;
        e.preventDefault();
        if (which === 'min') {
          pendingMin = Math.max(minUnitData, Math.min(pendingMax, pendingMin + delta));
        } else {
          pendingMax = Math.max(pendingMin, Math.min(maxUnitData, pendingMax + delta));
        }
        updateSliderUI();
      }
      handleMin.addEventListener('keydown', function(e) { handleSliderKey(e, 'min'); });
      handleMax.addEventListener('keydown', function(e) { handleSliderKey(e, 'max'); });

      var tResetBtn = document.createElement('button');
      tResetBtn.className = 'filter-edit-cancel';
      tResetBtn.textContent = 'Reset';
      tResetBtn.title = 'Reset to full range';

      var tCancelBtn = document.createElement('button');
      tCancelBtn.className = 'filter-edit-cancel';
      tCancelBtn.textContent = 'Cancel';

      var tApplyBtn = document.createElement('button');
      tApplyBtn.className = 'filter-edit-apply';
      tApplyBtn.textContent = 'Apply \u2713';

      var wasExpandedBeforeEdit = false;

      function enterTemporalEdit() {
        temporalFilterMode = 'edit';
        temporalFilterBtn.style.display = 'none';
        temporalEditControls.style.display = '';
        rangeSlider.style.display = '';
        // Expand collapsed chart so all bars are visible
        if (collapseCtrl) {
          wasExpandedBeforeEdit = collapseCtrl.isExpanded();
          collapseCtrl.setExpanded(true);
        }
        // Init from current filter (convert actual values back to indices) or full range
        var cur = hourFilterState[dr.source] && hourFilterState[dr.source][temporalField];
        if (cur) {
          pendingMin = Math.max(0, unitList.indexOf(cur.min));
          // cur.max is the last included bin; boundary = its index + 1
          var maxBinIdx = unitList.indexOf(cur.max);
          pendingMax = maxBinIdx >= 0 ? maxBinIdx + 1 : maxUnitData;
        } else {
          pendingMin = minUnitData;
          pendingMax = maxUnitData;
        }
        // Show data with all filters EXCEPT the hour filter being edited
        var view = chartViews[sectionKey];
        if (view) {
          var editData = dr.source
            ? getFilteredDataExcluding(dr.source, temporalField, 'hour')
            : rawData;
          vegaUpdateData(view, 'source', editData);
        }
        // Align slider with chart X-axis after layout settles
        requestAnimationFrame(function() {
          alignSliderToChart();
          updateSliderUI();
        });
        wrapper.style.cursor = 'pointer';
      }

      function exitTemporalEdit() {
        temporalFilterMode = 'view';
        temporalFilterBtn.style.display = '';
        temporalEditControls.style.display = 'none';
        rangeSlider.style.display = 'none';
        clearBarDimming();
        wrapper.style.cursor = '';
        // Restore collapse state
        if (collapseCtrl) {
          collapseCtrl.setExpanded(wasExpandedBeforeEdit);
        }
        // Restore filtered data
        var view = chartViews[sectionKey];
        if (view) {
          var filtered = dr.source ? getFilteredData(dr.source) : rawData;
          vegaUpdateData(view, 'source', filtered);
        }
      }

      // --- Bar click/drag interaction in filter edit mode ---
      // Returns bin index (into unitList) for a bar SVG element, or -1
      function binIndexFromBar(barEl) {
        var label = barEl.getAttribute('aria-label') || '';
        var dateStr = extractFieldFromLabel(label, temporalField);
        if (dateStr) {
          var barUnit = extractTimeUnit(new Date(dateStr), tu);
          return unitList.indexOf(barUnit);
        }
        var hMatch = label.match(/(\d{1,2}):00/);
        if (hMatch) return unitList.indexOf(parseInt(hMatch[1], 10));
        return -1;
      }

      var barDragAnchor = -1; // bin index where drag started

      // Find the bar path element from an event target (if inside a visible mark group)
      function barFromEvent(e) {
        var el = e.target;
        // el could be the path itself or a child; walk up to find a path with aria-label
        while (el && el !== wrapper) {
          if (el.nodeName === 'path' && el.getAttribute('aria-label') && el.closest('.mark-rect.role-mark')) {
            return el;
          }
          el = el.parentNode;
        }
        return null;
      }

      onBarMouseDown = function(e) {
        if (temporalFilterMode !== 'edit') return;
        var bar = barFromEvent(e);
        if (!bar) return;
        var idx = binIndexFromBar(bar);
        if (idx < 0) return;
        e.preventDefault();
        barDragAnchor = idx;
        pendingMin = idx;
        pendingMax = idx + 1;
        updateSliderUI();
      };

      onBarMouseMove = function(e) {
        if (barDragAnchor < 0) return;
        var bar = barFromEvent(e);
        if (!bar) return;
        var idx = binIndexFromBar(bar);
        if (idx < 0) return;
        pendingMin = Math.min(barDragAnchor, idx);
        pendingMax = Math.max(barDragAnchor, idx) + 1;
        updateSliderUI();
      };

      function onBarMouseUp() {
        barDragAnchor = -1;
      }

      // Bar click/drag listeners are attached after vegaEmbed completes (see .then() below)
      document.addEventListener('mouseup', onBarMouseUp);

      temporalFilterBtn.addEventListener('click', enterTemporalEdit);

      tResetBtn.addEventListener('click', function() {
        pendingMin = minUnitData;
        pendingMax = maxUnitData;
        updateSliderUI();
      });

      tCancelBtn.addEventListener('click', exitTemporalEdit);

      tApplyBtn.addEventListener('click', function() {
        temporalFilterMode = 'view';
        temporalFilterBtn.style.display = '';
        temporalEditControls.style.display = 'none';
        rangeSlider.style.display = 'none';
        clearBarDimming();
        if (pendingMin <= minUnitData && pendingMax >= maxUnitData) {
          clearHourFilter(dr.source, temporalField);
        } else if (pendingMin >= pendingMax) {
          // Empty selection — clear filter
          clearHourFilter(dr.source, temporalField);
        } else {
          // Convert boundary indices to actual bin values for filtering
          // Selected bins: [pendingMin, pendingMax) → values unitList[pendingMin]..unitList[pendingMax-1]
          var actualMin = unitList[pendingMin];
          var actualMax = unitList[pendingMax - 1];
          setHourFilter(dr.source, temporalField, actualMin, actualMax, tu);
        }
      });

      temporalEditControls.appendChild(tResetBtn);
      temporalEditControls.appendChild(tCancelBtn);
      temporalEditControls.appendChild(tApplyBtn);
      s.actions.insertBefore(temporalEditControls, s.actions.firstChild);

      rangeSlider.style.display = 'none';
      wrapper.parentNode.insertBefore(rangeSlider, wrapper.nextSibling);
    }

    function updateFilterBtnState() {
      if (filterBtn) {
        var hasFilter = filterState[dr.source] && filterState[dr.source][filterField];
        filterBtn.classList.toggle('filter-active', !!hasFilter);
      }
      if (temporalFilterBtn) {
        var hasHourFilter = hourFilterState[dr.source] && hourFilterState[dr.source][temporalField];
        temporalFilterBtn.classList.toggle('filter-active', !!hasHourFilter);
      }
    }

    var wasExpandedBeforeDiscreteEdit = false;

    function enterEditMode() {
      filterMode = 'edit';
      filterBtn.style.display = 'none';
      editControls.style.display = '';

      // Expand collapsed chart so all values are visible
      if (collapseCtrl) {
        wasExpandedBeforeDiscreteEdit = collapseCtrl.isExpanded();
        collapseCtrl.setExpanded(true);
      }

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

      // Show data with all filters EXCEPT the one being edited
      var view = chartViews[sectionKey];
      if (view) {
        var editData = dr.source
          ? getFilteredDataExcluding(dr.source, filterField, 'discrete')
          : rawData;
        vegaUpdateData(view, 'source', editData);
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

      // Restore collapse state
      if (collapseCtrl) collapseCtrl.setExpanded(wasExpandedBeforeDiscreteEdit);

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

      // Restore collapse state
      if (collapseCtrl) collapseCtrl.setExpanded(wasExpandedBeforeDiscreteEdit);

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
    var filterFieldType = filterField ? inferFieldType(rawData, filterField) : 'string';

    function renderChartWithSelection() {
      if (!pendingSelection) return;
      var svg = div.querySelector('svg');
      if (!svg) {
        if (++selectionRetries < 20) setTimeout(renderChartWithSelection, 100);
        return;
      }
      selectionRetries = 0;

      // Find all mark path/rect elements with aria-label containing filterField values
      var marks = svg.querySelectorAll('.mark-rect path, .mark-arc path');
      for (var i = 0; i < marks.length; i++) {
        var label = marks[i].getAttribute('aria-label') || '';
        var strValue = extractFieldFromLabel(label, filterField);
        if (strValue !== null) {
          // Coerce aria-label string back to the field's native type for correct key lookup
          var typed = coerceExtractedValue(strValue, filterFieldType);
          var key = distinctKey(typed);
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

        // Brush selection listener for non-timeUnit temporal charts
        if (temporalChannel && temporalField && !filterField && !hasTimeUnit) {
          var brushDebounce = null;
          result.view.addSignalListener('brush', function(name, value) {
            clearTimeout(brushDebounce);
            brushDebounce = setTimeout(function() {
              // Vega-Lite may key the range by the raw field or a timeUnit-derived
              // name (e.g. "hours_ts"). Try all keys and pick the array.
              var range = null;
              if (value) {
                for (var k in value) {
                  if (Array.isArray(value[k]) && value[k].length === 2) {
                    range = value[k];
                    break;
                  }
                }
              }
              if (range) {
                var min = +range[0], max = +range[1];
                if (isFinite(min) && isFinite(max) && min < max) {
                  setRangeFilter(dr.source, temporalField, min, max);
                  return;
                }
              }
              // Empty or cleared selection — remove range filter
              if (rangeFilterState[dr.source] && rangeFilterState[dr.source][temporalField]) {
                clearRangeFilter(dr.source, temporalField);
              }
            }, 80);
          });
        }

        // Attach bar click/drag listeners now that the DOM is ready
        if (onBarMouseDown) {
          wrapper.addEventListener('mousedown', onBarMouseDown);
          wrapper.addEventListener('mousemove', onBarMouseMove);
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

    var lastFilteredData = data; // Track last data to avoid unnecessary updates

    return function updateChart() {
      if (filterMode === 'edit' || temporalFilterMode === 'edit') return; // Don't update while editing

      // Brush-owning charts exclude own range filter to preserve full time context
      var filtered;
      if (temporalChannel && !hasTimeUnit && dr.source) {
        filtered = getFilteredDataExcluding(dr.source, temporalField, 'range');
      } else {
        filtered = dr.source ? getFilteredData(dr.source) : rawData;
      }
      if (!filtered) filtered = [];

      if (collapseCtrl && isHorizontalBar(cfg)) {
        var yField = (cfg.encoding.y || {}).field;
        if (yField) {
          var cats = countDistinct(filtered, yField);
          collapseCtrl.setLabel('Show all ' + cats + ' categories');
          collapseCtrl.setVisible(cats > CHART_COLLAPSE_THRESHOLD);
        }
      }

      // Skip data update if unchanged — preserves brush selection state
      var view = chartViews[sectionKey];
      if (view && filtered !== lastFilteredData) {
        lastFilteredData = filtered;
        vegaUpdateData(view, 'source', filtered);
      }
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
        var aInvalid = isNaN(at), bInvalid = isNaN(bt);
        if (aInvalid && bInvalid) result = 0;
        else if (aInvalid) result = 1;
        else if (bInvalid) result = -1;
        else result = at - bt;
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
      // Sort full dataset first, then truncate for display
      var displayData = allData.slice();
      if (sortState) {
        var sf = sortState.field, sa = sortState.ascending, st = sortTypes[sf] || 'string';
        displayData.sort(function(a, b) { return compareValues(a[sf], b[sf], st, sa); });
      }
      var totalRows = Math.min(displayData.length, MAX_ROWS);
      displayData = displayData.slice(0, totalRows);

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

      var needsCollapse = totalRows > INITIAL_ROWS + 2; // margin to avoid gradient over barely-clipped content
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
    for (var rsrc in rangeFilterState) {
      for (var rfield in rangeFilterState[rsrc]) {
        var range = rangeFilterState[rsrc][rfield];
        var tag2 = document.createElement('button');
        tag2.className = 'filter-tag';
        tag2.textContent = rfield + ': ' + formatTemporalRange(range.min, range.max);
        tag2.setAttribute('title', 'Remove time range filter');
        (function(s2, f2) {
          tag2.addEventListener('click', function() { clearRangeFilter(s2, f2); });
        })(rsrc, rfield);
        tags.appendChild(tag2);
      }
    }
    for (var hsrc in hourFilterState) {
      for (var hfield in hourFilterState[hsrc]) {
        var hrange = hourFilterState[hsrc][hfield];
        var htag = document.createElement('button');
        htag.className = 'filter-tag';
        var htu = hrange.timeUnit || 'hours';
        htag.textContent = hfield + ': ' + formatTimeUnitValue(hrange.min, htu) + ' \u2013 ' + formatTimeUnitValue(hrange.max, htu);
        htag.setAttribute('title', 'Remove time filter');
        (function(s3, f3) {
          htag.addEventListener('click', function() { clearHourFilter(s3, f3); });
        })(hsrc, hfield);
        tags.appendChild(htag);
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
  // LIST
  // ============================================================
  var LIST_INITIAL_ITEMS = 20;
  var LIST_MAX_ITEMS = 500;

  function mountList(s, section) {
    var cfg = section.list;
    if (!cfg) { appendError(s.body, 'No list config'); return null; }
    var dr = getDataResult(section);
    if (!dr.ok) { appendError(s.body, dr.error); return null; }

    var idField = cfg.id_field;
    if (!idField) { appendError(s.body, 'list requires id_field'); return null; }

    var layout = cfg.layout || 'cards';
    var titleField = cfg.title_field;
    var subtitleField = cfg.subtitle_field;
    var metaField = cfg.meta_field;
    var previewField = cfg.preview_field;
    var detailCfg = cfg.detail || null;
    var bodyField = detailCfg && detailCfg.body_field;
    var bodyFormat = (detailCfg && detailCfg.body_format) || 'text';

    var listContainer = document.createElement('div');
    listContainer.className = 'list-container';
    if (layout === 'rows') listContainer.classList.add('list-layout-rows');
    else if (layout === 'compact') listContainer.classList.add('list-layout-compact');
    s.body.appendChild(listContainer);

    var collapseCtrl = null;
    var currentDetailId = null; // id of item shown in detail view

    function getData() {
      return dr.source ? getFilteredData(dr.source) : dr.data || [];
    }

    function findItem(data, id) {
      for (var i = 0; i < data.length; i++) {
        if (data[i][idField] === id) return data[i];
      }
      return null;
    }

    function renderList() {
      listContainer.innerHTML = '';
      var data = getData();

      // Count display
      var countEl = document.createElement('div');
      countEl.className = 'list-count';
      var totalData = dr.source ? (datasets[dr.source] || []) : dr.data || [];
      if (data.length < totalData.length) {
        countEl.textContent = data.length + ' / ' + totalData.length + ' items';
      } else {
        countEl.textContent = data.length + ' items';
      }
      listContainer.appendChild(countEl);

      var displayData = data.slice(0, LIST_MAX_ITEMS);
      var wrapper = document.createElement('div');
      wrapper.className = 'list-items-wrapper';

      for (var i = 0; i < displayData.length; i++) {
        wrapper.appendChild(renderListItem(displayData[i]));
      }
      listContainer.appendChild(wrapper);

      // Collapse if many items
      var needsCollapse = displayData.length > LIST_INITIAL_ITEMS;
      if (needsCollapse && !collapseCtrl) {
        wrapper.classList.add('collapsed');
        wrapper.style.maxHeight = (LIST_INITIAL_ITEMS * 56) + 'px';
        wrapper.style.overflow = 'hidden';
        wrapper.style.position = 'relative';
        // Add gradient
        var grad = document.createElement('div');
        grad.style.cssText = 'position:absolute;bottom:0;left:0;right:0;height:60px;background:linear-gradient(transparent,white);pointer-events:none;';
        wrapper.appendChild(grad);

        collapseCtrl = addCollapseToggle(s.actions, wrapper,
          'Show all ' + displayData.length + ' items',
          function(exp) {
            wrapper.style.maxHeight = exp ? '' : (LIST_INITIAL_ITEMS * 56) + 'px';
            wrapper.style.overflow = exp ? '' : 'hidden';
            grad.style.display = exp ? 'none' : '';
          });
      } else if (collapseCtrl) {
        collapseCtrl.setLabel('Show all ' + displayData.length + ' items');
        collapseCtrl.setVisible(needsCollapse);
        if (!needsCollapse) {
          wrapper.style.maxHeight = '';
          wrapper.style.overflow = '';
        }
      }
    }

    function renderListItem(item) {
      var card = document.createElement('div');
      card.className = 'list-item-card';
      card.tabIndex = 0;
      card.setAttribute('role', 'button');

      var header = document.createElement('div');
      header.className = 'list-item-header';

      if (titleField && item[titleField] != null) {
        var title = document.createElement('div');
        title.className = 'list-item-title';
        title.textContent = formatCell(item[titleField]);
        header.appendChild(title);
      }
      if (metaField && item[metaField] != null) {
        var meta = document.createElement('div');
        meta.className = 'list-item-meta';
        meta.textContent = formatCell(item[metaField]);
        header.appendChild(meta);
      }
      card.appendChild(header);

      if (subtitleField && item[subtitleField] != null) {
        var sub = document.createElement('div');
        sub.className = 'list-item-subtitle';
        sub.textContent = formatCell(item[subtitleField]);
        card.appendChild(sub);
      }
      if (previewField && item[previewField] != null) {
        var preview = document.createElement('div');
        preview.className = 'list-item-preview';
        preview.textContent = formatCell(item[previewField]);
        card.appendChild(preview);
      }

      var itemId = item[idField];
      function openDetail() { showDetail(itemId); }
      card.addEventListener('click', openDetail);
      card.addEventListener('keydown', function(e) {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openDetail(); }
      });

      return card;
    }

    function showDetail(itemId) {
      currentDetailId = itemId;
      listContainer.innerHTML = '';

      var data = getData();
      var item = findItem(data, itemId);
      if (!item) {
        // Item filtered out — fall back to list
        currentDetailId = null;
        renderList();
        return;
      }

      var view = document.createElement('div');
      view.className = 'list-detail-view';

      // Back button
      var backBtn = document.createElement('button');
      backBtn.className = 'list-detail-back';
      backBtn.innerHTML = '\u2190 Back to list';
      backBtn.addEventListener('click', function() {
        currentDetailId = null;
        renderList();
      });
      view.appendChild(backBtn);

      // Title
      if (titleField && item[titleField] != null) {
        var detailTitle = document.createElement('div');
        detailTitle.className = 'list-detail-title';
        detailTitle.textContent = formatCell(item[titleField]);
        view.appendChild(detailTitle);
      }

      // Detail fields
      if (detailCfg && detailCfg.fields && detailCfg.fields.length > 0) {
        var fieldsGrid = document.createElement('div');
        fieldsGrid.className = 'list-detail-fields';
        for (var i = 0; i < detailCfg.fields.length; i++) {
          var fDef = detailCfg.fields[i];
          var label = document.createElement('div');
          label.className = 'list-detail-field-label';
          label.textContent = fDef.title || fDef.field;
          fieldsGrid.appendChild(label);

          var val = document.createElement('div');
          val.className = 'list-detail-field-value';
          val.textContent = formatCell(item[fDef.field]);
          fieldsGrid.appendChild(val);
        }
        view.appendChild(fieldsGrid);
      }

      // Body — determine format per item: use item.body_format if present, else section config
      if (bodyField && item[bodyField] != null) {
        var itemFormat = bodyFormat;
        if (item.body_format === 'html') itemFormat = 'sanitized_html';
        else if (item.body_format === 'text') itemFormat = 'text';

        var bodyEl = document.createElement('div');
        if (itemFormat === 'sanitized_html') {
          bodyEl.className = 'list-detail-body-html';
          bodyEl.innerHTML = item[bodyField]; // already sanitized server-side
        } else {
          bodyEl.className = 'list-detail-body';
          bodyEl.textContent = item[bodyField];
        }
        view.appendChild(bodyEl);
      }

      listContainer.appendChild(view);
    }

    // Initial render
    renderList();

    return function updateList() {
      if (currentDetailId !== null) {
        // Check if current detail item still passes filter
        var data = getData();
        if (!findItem(data, currentDetailId)) {
          currentDetailId = null;
          renderList();
        }
        // else: detail stays open (item still visible)
      } else {
        renderList();
      }
    };
  }

  // ============================================================
  // MARKDOWN SECTIONS
  // ============================================================
  var MD_DEFAULT_MAX_ROWS = 100;

  // Configure marked once at startup
  if (typeof marked !== 'undefined' && marked.setOptions) {
    marked.setOptions({ gfm: true, breaks: false });
  }

  function slugify(text) {
    return String(text).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  }

  // All markdown section contentEls (for heading scanning by TOC)
  var mdContentEls = [];
  // TOC config from the one section that declares toc_levels
  var mdTocConfig = null;

  // Module-scope handles for TOC lifecycle cleanup
  var mdTocObserver = null;
  var mdTocScrollHandler = null;

  function mountMarkdown(s, section) {
    var mdConfig = section.markdown;
    if (!mdConfig) { appendError(s.body, 'Missing markdown config'); return null; }

    var contentEl = document.createElement('div');
    contentEl.className = 'markdown-body';
    s.body.appendChild(contentEl);

    var sectionId = section.id || ('md-' + mdContentEls.length);

    // Register contentEl so TOC can scan headings from all markdown sections
    mdContentEls.push({ contentEl: contentEl, sectionId: sectionId });

    // If this section declares toc_levels, it controls the TOC config
    var tocLevels = mdConfig.toc_levels;
    if (Array.isArray(tocLevels) && tocLevels.length > 0 && !mdTocConfig) {
      mdTocConfig = {
        levels: tocLevels,
        side: mdConfig.toc_side || 'left',
      };
    }

    var mdMaxRows = (mdConfig.max_rows && mdConfig.max_rows > 0) ? mdConfig.max_rows : MD_DEFAULT_MAX_ROWS;
    var isDynamic = !!mdConfig.content_field;
    var lastRaw = null;

    function getContent() {
      if (mdConfig.content) return mdConfig.content;
      if (mdConfig.content_field) {
        var dataResult = getDataResult(section);
        if (!dataResult.ok) return 'Error: ' + dataResult.error;
        var data = getFilteredData(dataResult.source) || dataResult.data;
        if (data && data.length > 0) {
          var parts = [];
          var truncated = false;
          for (var i = 0; i < data.length && parts.length < mdMaxRows; i++) {
            var val = data[i][mdConfig.content_field];
            if (val != null) parts.push(String(val));
          }
          if (data.length > mdMaxRows) truncated = true;
          var result = parts.join('\n\n');
          if (truncated) {
            result += '\n\n---\n\n*Content truncated: showing ' + parts.length + ' of ' + data.length + ' rows (max_rows: ' + mdMaxRows + ')*';
          }
          return result;
        }
        return '';
      }
      return '';
    }

    function renderMarkdown() {
      var raw = getContent();
      // XSS prevention: sanitize rendered HTML with DOMPurify
      if (typeof marked !== 'undefined' && marked.parse) {
        var html = marked.parse(raw);
        if (typeof DOMPurify !== 'undefined') {
          contentEl.innerHTML = DOMPurify.sanitize(html);
        } else {
          contentEl.textContent = raw;
          appendError(s.body, 'Markdown sanitizer (DOMPurify) unavailable');
          return;
        }
      } else {
        contentEl.textContent = raw;
      }

      // Syntax highlighting with error handling
      if (typeof hljs !== 'undefined') {
        var codeBlocks = contentEl.querySelectorAll('pre code');
        for (var i = 0; i < codeBlocks.length; i++) {
          try { hljs.highlightElement(codeBlocks[i]); }
          catch (e) { appendError(s.body, 'Syntax highlighting failed: ' + e.message); }
        }
      }

      // Link target configuration (external links only)
      if (mdConfig.link_target) {
        var links = contentEl.querySelectorAll('a[href]');
        for (var j = 0; j < links.length; j++) {
          var href = links[j].getAttribute('href') || '';
          if (href.startsWith('#') || href.startsWith('mailto:') || href.startsWith('tel:')) continue;
          links[j].setAttribute('target', mdConfig.link_target);
          if (mdConfig.link_target === '_blank') {
            links[j].setAttribute('rel', 'noopener noreferrer');
          }
        }
      }
    }

    renderMarkdown();
    lastRaw = getContent();

    return function updateMarkdown() {
      if (!isDynamic) return; // static content never changes
      var raw = getContent();
      if (raw === lastRaw) return;
      lastRaw = raw;
      renderMarkdown();
      buildMarkdownToc();
    };
  }

  // Build a single global TOC scanning headings from ALL markdown sections
  function buildMarkdownToc() {
    // Clean up previous observer and scroll listener
    if (mdTocObserver) { mdTocObserver.disconnect(); mdTocObserver = null; }
    if (mdTocScrollHandler) { window.removeEventListener('scroll', mdTocScrollHandler); mdTocScrollHandler = null; }

    if (!mdTocConfig || mdContentEls.length === 0) return;

    // Remove all previous TOC sidebars
    var prevAll = document.querySelectorAll('.markdown-toc-sidebar');
    prevAll.forEach(function(el) { el.remove(); });
    document.body.classList.remove('has-md-toc-left', 'has-md-toc-right');

    // Use levels and side from the TOC-declaring section
    var levels = mdTocConfig.levels.slice().sort(function(a, b) { return a - b; });
    var tocSide = mdTocConfig.side;
    var minLevel = levels[0];
    var selector = levels.map(function(l) { return 'h' + l; }).join(',');

    // Build the sidebar
    var tocNav = document.createElement('nav');
    tocNav.className = 'markdown-toc-sidebar md-toc-' + tocSide;
    tocNav.setAttribute('aria-label', 'Table of contents');
    var tocTitle = document.createElement('div');
    tocTitle.className = 'toc-title';
    tocTitle.textContent = 'Contents';
    tocNav.appendChild(tocTitle);
    var tocList = document.createElement('ul');
    tocList.className = 'toc-list';

    var tocLinks = [];
    var clickedIndex = -1; // click override to prevent observer from jumping

    // First entry: page title → scroll to top
    var pageH1 = document.querySelector('body > h1');
    if (pageH1) {
      var topLi = document.createElement('li');
      var topA = document.createElement('a');
      topA.href = '#';
      topA.textContent = pageH1.textContent;
      topA.addEventListener('click', function(e) {
        e.preventDefault();
        clickedIndex = 0;
        setActive(0);
        window.scrollTo({ top: 0, behavior: 'smooth' });
      });
      topLi.appendChild(topA);
      tocList.appendChild(topLi);
      tocLinks.push({ el: topA, heading: pageH1 });
    }

    // Collect headings from ALL markdown sections in order
    for (var s = 0; s < mdContentEls.length; s++) {
      var regEntry = mdContentEls[s];
      var headings = regEntry.contentEl.querySelectorAll(selector);
      var slugCounts = {}; // track slug collisions within section

      for (var i = 0; i < headings.length; i++) {
        var h = headings[i];

        // Deterministic heading ID: {sectionId}-{slug}[-{n}]
        var baseSlug = slugify(h.textContent) || 'heading';
        var fullSlug = regEntry.sectionId + '-' + baseSlug;
        var count = slugCounts[fullSlug] || 0;
        slugCounts[fullSlug] = count + 1;
        var id = count === 0 ? fullSlug : fullSlug + '-' + count;
        h.id = id;

        var level = parseInt(h.tagName.charAt(1), 10);
        var indent = level - minLevel;

        var li = document.createElement('li');
        li.style.paddingLeft = (indent * 0.75) + 'rem';
        var a = document.createElement('a');
        a.href = '#' + id;
        a.textContent = h.textContent;
        a.addEventListener('click', (function(targetId, idx) {
          return function(e) {
            e.preventDefault();
            clickedIndex = idx;
            setActive(idx);
            var target = document.getElementById(targetId);
            if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
          };
        })(id, tocLinks.length));
        li.appendChild(a);
        tocList.appendChild(li);
        tocLinks.push({ el: a, heading: h });
      }
    }

    tocNav.appendChild(tocList);
    document.body.appendChild(tocNav);
    document.body.classList.add('has-md-toc-' + tocSide);

    // Scroll-based active highlight with click override
    function setActive(idx) {
      for (var j = 0; j < tocLinks.length; j++) {
        tocLinks[j].el.classList.toggle('toc-active', j === idx);
        if (j === idx) {
          tocLinks[j].el.setAttribute('aria-current', 'location');
        } else {
          tocLinks[j].el.removeAttribute('aria-current');
        }
      }
    }

    if (typeof IntersectionObserver !== 'undefined' && tocLinks.length > 0) {
      mdTocObserver = new IntersectionObserver(function(entries) {
        if (clickedIndex >= 0) return; // click override active
        entries.forEach(function(entry) {
          if (entry.isIntersecting) {
            for (var k = 0; k < tocLinks.length; k++) {
              if (tocLinks[k].heading === entry.target) { setActive(k); break; }
            }
          }
        });
      }, { rootMargin: '-80px 0px -60% 0px' });
      for (var j = 0; j < tocLinks.length; j++) mdTocObserver.observe(tocLinks[j].heading);

      // Clear click override after scroll settles
      var clickClearTimer = null;
      mdTocScrollHandler = function() {
        if (clickedIndex < 0) return;
        clearTimeout(clickClearTimer);
        clickClearTimer = setTimeout(function() { clickedIndex = -1; }, 300);
      };
      window.addEventListener('scroll', mdTocScrollHandler, { passive: true });
    }

    setActive(0);
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
        case 'list': updateFn = mountList(s, section); break;
        case 'markdown': updateFn = mountMarkdown(s, section); break;
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

  // Build markdown TOC after all sections are rendered
  buildMarkdownToc();

  // ============================================================
  // TABLE OF CONTENTS (sidebar)
  // ============================================================
  if (spec.toc) {
    var tocNav = document.createElement('nav');
    tocNav.className = 'toc-sidebar';

    var tocTitle = document.createElement('div');
    tocTitle.className = 'toc-title';
    tocTitle.textContent = 'Contents';
    tocNav.appendChild(tocTitle);

    var tocList = document.createElement('ul');
    tocList.className = 'toc-list';

    var clickedIndex = -1; // track manual click to override scroll-based highlight

    spec.sections.forEach(function(section, i) {
      var sectionId = section.id || ('section-' + i);
      var li = document.createElement('li');
      var a = document.createElement('a');
      a.href = '#' + sectionId;
      a.textContent = section.title;
      a.addEventListener('click', function(e) {
        e.preventDefault();
        clickedIndex = i;
        setTocActive(i);
        var target = document.getElementById(sectionId);
        if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
      });
      li.appendChild(a);
      tocList.appendChild(li);
    });

    tocNav.appendChild(tocList);
    document.body.appendChild(tocNav);
    document.body.classList.add('has-toc');

    // Highlight active section using IntersectionObserver (avoids synchronous offsetTop)
    var tocLinks = tocList.querySelectorAll('a');

    function setTocActive(specIndex) {
      for (var j = 0; j < tocLinks.length; j++) {
        tocLinks[j].classList.toggle('toc-active', j === specIndex);
      }
    }

    if (typeof IntersectionObserver !== 'undefined') {
      var activeTocIndex = 0;
      var observer = new IntersectionObserver(function(entries) {
        // Only update from scroll if no click override is active
        if (clickedIndex >= 0) return;
        entries.forEach(function(entry) {
          if (entry.isIntersecting) {
            var idx = Number(entry.target.dataset.tocIndex);
            if (!isNaN(idx)) { activeTocIndex = idx; setTocActive(idx); }
          }
        });
      }, { rootMargin: '-80px 0px -60% 0px' });

      spec.sections.forEach(function(section, i) {
        var el = document.getElementById(section.id || ('section-' + i));
        if (el) {
          el.dataset.tocIndex = String(i);
          observer.observe(el);
        }
      });

      // Clear click override after scroll settles
      var clickClearTimer = null;
      window.addEventListener('scroll', function() {
        if (clickedIndex < 0) return;
        clearTimeout(clickClearTimer);
        clickClearTimer = setTimeout(function() { clickedIndex = -1; }, 300);
      }, { passive: true });
    }

    setTocActive(0);
  }

})();
