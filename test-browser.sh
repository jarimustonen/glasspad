#!/bin/bash
# test-browser.sh — Run JavaScript in Brave Browser via AppleScript
# Usage:
#   ./test-browser.sh eval 'document.title'
#   ./test-browser.sh page 'typeof vegaEmbed'     # runs in PAGE context (sees page globals)
#   ./test-browser.sh nav  URL                     # navigate to URL
#   ./test-browser.sh click SELECTOR               # click an element
#   ./test-browser.sh bar-click INDEX              # click bar INDEX in #timeline (filter must be active)
#   ./test-browser.sh filter-on                    # enter timeline filter mode
#   ./test-browser.sh filter-label                 # read slider label
#   ./test-browser.sh deploy                       # rebuild + restart + create pad + navigate

PROBE_INIT='if(!document.getElementById("__probe")){var p=document.createElement("div");p.id="__probe";document.body.appendChild(p);}'

run_js() {
  osascript -l JavaScript -e "
    var brave = Application('Brave Browser');
    var tab = brave.windows[0].activeTab;
    tab.execute({javascript: $(printf '%s' "$1" | python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))")});
  " 2>&1
}

# Run JS in the PAGE context (not isolated world) via injected <script>
run_page_js() {
  local escaped
  escaped=$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')
  run_js "${PROBE_INIT} var s=document.createElement('script'); s.textContent=\"${escaped}\"; document.body.appendChild(s); document.getElementById('__probe').dataset.r"
}

case "${1:-help}" in
  eval)
    run_js "$2"
    ;;
  page)
    # Wrap user code to store result in probe
    run_page_js "document.getElementById('__probe').dataset.r = '' + ($2);"
    ;;
  nav)
    run_js "location.href='$2'; 'navigating'"
    sleep 3
    run_js "document.title + ' | ' + location.href"
    ;;
  click)
    run_js "var el=document.querySelector('$2'); el ? (el.click(),'clicked') : 'not found: $2'"
    ;;
  filter-on)
    run_js "var btn=document.querySelector('#timeline .filter-edit-btn'); btn?(btn.click(),'filter activated'):'no filter btn'"
    sleep 0.5
    run_js "var l=document.querySelector('.hour-range-label'); l?'label: '+l.textContent:'no label'"
    ;;
  filter-label)
    run_js "var l=document.querySelector('.hour-range-label'); l?l.textContent:'no label'"
    ;;
  bar-click)
    IDX="${2:-0}"
    run_page_js "var groups=document.querySelectorAll('.mark-rect.role-mark'); var vis=groups[groups.length-1]; var bar=vis.querySelectorAll('path[aria-label]')[${IDX}]; if(!bar){document.getElementById('__probe').dataset.r='no bar at index ${IDX}';} else {bar.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true})); document.getElementById('__probe').dataset.r='label: '+document.querySelector('.hour-range-label').textContent+' | bar: '+bar.getAttribute('aria-label').substring(0,45);}"
    ;;
  bar-drag)
    FROM="${2:-0}"
    TO="${3:-0}"
    run_page_js "var groups=document.querySelectorAll('.mark-rect.role-mark'); var vis=groups[groups.length-1]; var bars=vis.querySelectorAll('path[aria-label]'); bars[${FROM}].dispatchEvent(new MouseEvent('mousedown',{bubbles:true})); bars[${TO}].dispatchEvent(new MouseEvent('mousemove',{bubbles:true})); document.dispatchEvent(new MouseEvent('mouseup',{bubbles:true})); document.getElementById('__probe').dataset.r='label: '+document.querySelector('.hour-range-label').textContent;"
    ;;
  errors)
    run_js "var errs=document.querySelectorAll('.chart-error,.error'); var r=[]; errs.forEach(function(e){r.push(e.textContent);}); r.length?r.join(' | '):'no errors'"
    ;;
  deploy)
    echo "Building..."
    pkill -f "target/debug/glasspad" 2>/dev/null
    sleep 1
    cargo build 2>&1 | tail -1
    cargo run -- serve > /dev/null 2>&1 &
    disown
    sleep 3
    echo "Creating pad..."
    RESULT=$(cargo run -- create -f history/examples/email-dashboard.yaml --data emails=history/examples/email-search-results.json 2>&1 | tail -1)
    ID=$(echo "$RESULT" | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['id'])")
    URL="http://localhost:3000/$ID"
    echo "Navigating to $URL"
    run_js "location.href='${URL}'; 'ok'"
    sleep 3
    run_js "'loaded: ' + document.title + ' | bars: ' + document.querySelectorAll('.mark-rect path').length"
    echo "Pad URL: $URL"
    ;;
  help|*)
    echo "Usage: $0 <command> [args]"
    echo "  eval  'js'      — run JS in isolated context"
    echo "  page  'js'      — run JS in page context (sees vegaEmbed etc)"
    echo "  nav   URL       — navigate and wait"
    echo "  click SELECTOR  — click element"
    echo "  filter-on       — enter timeline filter mode"
    echo "  filter-label    — read slider label"
    echo "  bar-click INDEX — click bar in filter mode"
    echo "  bar-drag F T    — drag from bar F to bar T"
    echo "  errors          — show page errors"
    echo "  deploy          — rebuild + restart + create pad + navigate"
    ;;
esac
