# Glasspad — Työsuunnitelma

AI scratchpad for rich data views.

## Status

Vaihe 7 valmis (suodatus, aikafiltteri, TOC, mbox-tuki). Review v2+v3 korjaukset + temporal chart UX-parannukset tehty. Seuraavaksi: vaihe 8 (list-näkymä).

Sopimus: `04-spec-contract.md`
Reviews: `review-architecture-v1.md`, `review-spec-contract-impl.md`, `review-integration-v1.md`, `review-client-rendering.md`, `review-ui-improvements.md`, `review-cross-filtering.md`, `review-cross-filtering-v2.md`, `review-cross-filtering-v3.md`

---

## Vaihe 1–6: ✅ Valmis

## Vaihe 7: Interaktiivinen suodatus ✅

- [x] 7.1 Filter state -malli (per dataset, per field, Object.create(null))
- [x] 7.2 Filter edit mode: 🔍 nappi → All/None/Cancel/Apply kontrollit
- [x] 7.3 Multi-select: klikkaa palkkeja valitaksesi/poistaaksesi, DOM-pohjainen opacity
- [x] 7.4 Suodatetun datan syöttö kaikkiin saman source:n sectioneihin (vega.changeset)
- [x] 7.5 Filter bar (tagit, reset all, pulse vain lisäyksessä)
- [x] 7.6 Section-tilan säilyminen (taulukko-sort, collapse-tila päivittyvät dynaamisesti)
- [x] 7.7 Filtered cache (memoized per onFilterChange-sykli)
- [x] 7.8 Step-based chart height + CSS min-height vakaus
- [x] 7.9 Count-akseli: kokonaislukutickkit (labelExpr + conditional tick/grid color)
- [x] 7.10 CLI --data säilyttää source-identiteetin (cross-filtering toimii)
- [x] 7.11 Temporal-akselin kiinteä domain (ghost layer lukitsee akselit suodatettaessa)
- [x] 7.12 Aikavälivalinta (brush/interval selection non-timeUnit temporal chartissa)
- [x] 7.13 Aikayksikkö range filter (dual-handle slider, dimming, indeksipohjainen)
- [x] 7.14 Range filter tila (rangeFilterState + hourFilterState, getFilteredData)
- [x] 7.15 Auto-expand collapsed chart filter edit modessa
- [x] 7.16 TOC-sivupalkki (toc: true, IntersectionObserver, scroll-to-section)
- [x] 7.17 Timezone-tuki (spec.timezone: utc/local, getHourOfDate, formatTemporalRange)
- [x] 7.18 Mbox/EML -parseri (mail-parser, CLI --data emails=inbox.mbox)
- [x] 7.19 Email-testidata (20 synteettistä viestiä, email-dashboard.yaml)

Review-korjaukset (v2+v3):
- [x] Sort full dataset before truncating table rows
- [x] Boundary-aware extractFieldFromLabel (substring-bugi)
- [x] Type-aware filter selection (coerceExtractedValue)
- [x] Retry limit renderChartWithSelection (max 20)
- [x] Invalid dates → end in temporal sort
- [x] Edit mode context preservation (getFilteredDataExcluding)
- [x] Brush visual clear on Reset all
- [x] Block updates during temporal edit mode
- [x] "None" = empty allowed set, not clear filter
- [x] getFilteredDataExcluding cached per filter cycle
- [x] TimeUnit-generic slider (hours/day/date/month/year/yearmonthdate)
- [x] Slider bar-center alignment → korvattu: midpoint axis labels + N+1 boundary slider
- [x] Mbox read bytes not string (UTF-8 safe)
- [x] Missing mbox fields → CellValue::Null
- [x] Slider handle z-index (viimeksi tartuttu päällimmäisenä)
- [x] Focus-visible CSS kaikille painikkeille
- [x] Filter tag max-width + ellipsis
- [x] datasetHasField tarkistaa kaikki rivit
- [x] touchcancel handler slider dragissa
- [x] Slider keyboard accessibility (ARIA, arrow keys)
- [x] Table collapse gradient threshold +2
- [x] Table overflow-x: auto
- [x] SVG favicon
- [x] Axis labels centered under bars (midpoint timestamps via axis.values)
- [x] Slider N+1 boundary stops (half-open interval [min, max) model)
- [x] Bar click/drag selection in temporal filter mode
- [x] extractFieldFromLabel: handle timeUnit suffix in aria-labels
- [x] Browser test automation (test-browser.sh, osascript + Brave)
- [x] GUI debugging guide (AGENTS-GUI-DEBUGGING.md)

---

## Vaihe 8: Rikkaat datanäkymät (list) ⬜

> Ref: `04-spec-contract.md` §2 (list), `07-arch-rich-data-views.md`

- [ ] 8.1 List-section renderöinti (cards, rows, compact)
- [ ] 8.2 `id_field` pakollinen, validointi
- [ ] 8.3 Detail-näkymä (replace-moodi)
- [ ] 8.4 `body_format: text` / `sanitized_html` renderöinti
- [ ] 8.5 Detail → back-navigaatio
- [ ] 8.6 List reagoi suodatuksiin

## Vaihe 9: Kaksisuuntaiset toiminnot ⬜

> Ref: `04-spec-contract.md` §7, `08-arch-bidirectional-actions.md`

- [ ] 9.1 Completion-endpoint: `POST /api/pads/:id/complete`
- [ ] 9.2 `GET /api/pads/:id/completion` (CLI pollaa)
- [ ] 9.3 Action-painikkeet detail-näkymässä
- [ ] 9.4 `row_actions` taulukossa
- [ ] 9.5 Done-painike + Cancel-painike
- [ ] 9.6 Pending actions JS-tilassa
- [ ] 9.7 `--wait` CLI-lippu (blocking, timeout)
- [ ] 9.8 Pad lukitaan completionin jälkeen (409)

## Vaihe 10: Viimeistely ⬜

- [ ] 10.1 PID-tiedosto `~/.glasspad/server.pid`
- [ ] 10.2 `glasspad stop` -komento
- [ ] 10.3 `GLASSPAD_PORT` ympäristömuuttuja
- [ ] 10.4 Skill-päivitys (filter edit mode, --data, sort, tooltip)
- [ ] 10.5 README päivitys
- [ ] 10.6 `cargo install` ja testaus toisessa repossa

## Vaihe 11: MCP-integraatio ⬜

- [ ] 11.1 MCP-serveri: create_pad, update_pad, list_pads, delete_pad
- [ ] 11.2 MCP: wait_for_completion (blocking tool)
- [ ] 11.3 Testaus Claude Code -ympäristössä

## Tulevaisuus (ei aikataulua)

- [ ] Refaktori: SVG DOM scraping → Vega signals/params (filter selection opacity, slider alignment, bar dimming)
- [ ] Aikafiltteri: drill-down granulaarisuus (viikko→päivä→tunti toggle-painike)
- [ ] Aikafiltteri: konfiguroitava viikon ensimmäinen päivä (ma/su)
- [ ] Refaktori: mountChart()-funktion pilkkominen (buildSpec, filterControls, brushSync, collapse)
- [ ] Mark schema: tuki object-muodolle (Rust: `serde_json::Value`, nyt `String`)
- [ ] Unified filter state model (filterState + rangeFilterState + hourFilterState → yksi rakenne)
- [ ] OpenClaw-päätelaite → `08 §Tulevaisuus`
- [ ] Columnar Dataset (Vec<Row> → headers + Vec<Vec<CellValue>>)
- [ ] Fetch-endpoint isoille dataseteille
- [ ] Detail-moodit: overlay, fullscreen
- [ ] Deprecated-kenttien normalisointi (data:→datasets:, chart.data→inline_data)
- [ ] A2UI-yhteensopivuus
- [ ] Docker-image
- [ ] SQLite-persistenssi
- [ ] API: multipart upload (nyt CLI injektoi inline)

---

## Rinnakkaisuusanalyysi

```
Vaiheet 1–7: ✅ Valmis
    │
    ├──────────────────────┐
    │                      │
Vaihe 8: List          Vaihe 9: Toiminnot     ← RINNAKKAIN
    │                      │
    ├──────────────────────┘
    │
Vaihe 10–11: Viimeistely, MCP
```
