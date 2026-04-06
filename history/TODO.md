# Glasspad — Työsuunnitelma

AI scratchpad for rich data views.

## Status

Client-side renderöinti valmis, UX-korjaukset tehty. Seuraavaksi: interaktiivinen suodatus.

Sopimus: `04-spec-contract.md`
Reviews: `review-architecture-v1.md`, `review-spec-contract-impl.md`, `review-integration-v1.md`, `review-client-rendering.md`, `review-ui-improvements.md`

---

## Vaihe 1: Tutkimus ✅
## Vaihe 2: PoC ✅
## Vaihe 3: Spec contract ✅
## Vaihe 4: Turvallisuus ✅
## Vaihe 5: Data layer ✅

## Vaihe 6: Client-side renderöinti ✅

- [x] 6.1 Spec + datasets JSON selaimeen (safe script tags)
- [x] 6.2 JS-moduuli: parsii spec + datasets, renderöi kaikki sectionit
- [x] 6.3 Chart: vegaEmbed, view-instanssit tallennettu chartViews-registriin
- [x] 6.4 Table: DOM-pohjainen, thead kerran + tbody päivitetään sortilla
- [x] 6.5 Stats: aggregaatiot client-sidessa (count, distinct, sum, avg, min, max, where)
- [x] 6.6 Renderer.rs: HTML-runko + include_str! JS/CSS erillisistä tiedostoista
- [x] 6.7 Vakio section-rakenne: createSectionCard → {card, body, actions}
- [x] 6.8 Collapsible charts/tables: show more/less toggle, gradient fade
- [x] 6.9 Taulukon sarakesorttaus: asc→desc→original, tyyppikohtainen (number/string/temporal/boolean)
- [x] 6.10 SortType enum schemassa (validoidaan parse-vaiheessa)
- [x] 6.11 Tooltips kaikissa charteissa (normalizeMark)
- [x] 6.12 Dynaaminen korkeus horizontal bar charteille
- [x] 6.13 Auto-span: taulukot + paljon kategorioita → koko leveys
- [x] 6.14 Accessibility: aria-sort, aria-expanded, aria-controls, button-sort, focus-visible
- [x] 6.15 Docs päivitetty kanoniseen schemaan
- [x] 6.16 HTML-sanitoija (ammonia) body_format: sanitized_html valmis käyttöön

Puuttuu vielä:
- [ ] 5.7 API: multipart upload (nyt CLI injektoi inline)

---

## Vaihe 7: Interaktiivinen suodatus ⬜

> Ref: `04-spec-contract.md` §6, `06-arch-interactive-filtering.md`
> Pohja valmis: client-side renderöinti, chartViews-registry, vakio section-rakenne

- [ ] 7.1 Filter state -malli (per dataset, per field, Set<arvo>)
- [ ] 7.2 Chart-klikkaus → toggle filter (interactive_filter.field)
- [ ] 7.3 Suodatetun datan syöttö kaikkiin saman source:n sectioneihin
- [ ] 7.4 Kaikkien sectionien uudelleenrenderöinti suodatetulla datalla
- [ ] 7.5 Filter bar (kelluva, tagit, reset-nappi)
- [ ] 7.6 Pulse-animaatio kun suodatus muuttuu
- [ ] 7.7 Section-tilan säilyminen (taulukko-sort, collapse-tila)
- [ ] 7.8 Testaus analytics-esimerkkidatalla

## Vaihe 8: Rikkaat datanäkymät (list) ⬜                  ← rinnastettavissa vaiheen 7 kanssa

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
- [ ] 10.4 Skill-päivitys (uusi schema, --data, sort, tooltip)
- [ ] 10.5 README päivitys
- [ ] 10.6 `cargo install` ja testaus toisessa repossa

## Vaihe 11: MCP-integraatio ⬜

- [ ] 11.1 MCP-serveri: create_pad, update_pad, list_pads, delete_pad
- [ ] 11.2 MCP: wait_for_completion (blocking tool)
- [ ] 11.3 Testaus Claude Code -ympäristössä

## Tulevaisuus (ei aikataulua)

- [ ] OpenClaw-päätelaite → `08 §Tulevaisuus`
- [ ] Columnar Dataset (Vec<Row> → headers + Vec<Vec<CellValue>>)
- [ ] Fetch-endpoint isoille dataseteille
- [ ] Advanced filters -paneeli
- [ ] Detail-moodit: overlay, fullscreen
- [ ] Deprecated-kenttien normalisointi (data:→datasets:, chart.data→inline_data)
- [ ] A2UI-yhteensopivuus
- [ ] Docker-image
- [ ] SQLite-persistenssi

---

## Rinnakkaisuusanalyysi

```
Vaiheet 1–6: ✅ Tehty
    │
    ├──────────────────────┐
    │                      │
Vaihe 7: Suodatus      Vaihe 8: List              ← RINNAKKAIN
    │                      │
    ├──────────────────────┘
    │
Vaihe 9: Toiminnot (--wait)
    │
Vaihe 10–11: Viimeistely, MCP
```
