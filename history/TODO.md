# Glasspad — Työsuunnitelma

AI scratchpad for rich data views.

## Status

Spec contract toteutettu, data layer toimii, integroitu API+CLI+renderer.
Seuraavaksi: client-side renderöinti → interaktiivinen suodatus.

Sopimus: `04-spec-contract.md`
Reviews: `review-architecture-v1.md`, `review-spec-contract-impl.md`, `review-integration-v1.md`

---

## Vaihe 1: Tutkimus ✅

- [x] 1.1 Markkina- ja teknologiakatsaus → `01-research-landscape.md`
- [x] 1.2 Integraatiomalli → `02-design-integration-model.md`
- [x] 1.3 Teknologiavalinnat → `03-design-tech-choices.md` (Rust + Axum + Clap)
- [x] 1.4 Demo-skenaario → `04-design-demo-scenario.md`
- [x] 1.5 Arkkitehtuurisuunnitelmat → `05–08`, ref `09–10`, roadmap `11`
- [x] 1.6 Architecture review → `review-architecture-v1.md`
- [x] 1.7 Spec contract → `04-spec-contract.md`

## Vaihe 2: PoC ✅

- [x] 2.1 Rust-projekti, Axum-serveri, in-memory storage
- [x] 2.2 CRUD API (POST/GET/PUT/DELETE /api/pads)
- [x] 2.3 YAML → HTML renderöinti (chart, table, stats)
- [x] 2.4 CLI (create, list, open, docs, skill)
- [x] 2.5 Auto-start serveri, skill --install-claude

## Vaihe 3: Spec contract -toteutus ✅

- [x] 3.1 `spec_version: 1` pakolliseksi
- [x] 3.2 `datasets:` top-level, `deny_unknown_fields`
- [x] 3.3 `inline_data:` section-tasolla
- [x] 3.4 `interactive_filter: { field: x }` kanoninen muoto
- [x] 3.5 Section `id:` -kenttä (pakollinen interaktiivisille)
- [x] 3.6 Stats-schema: `stats.items` + aggregaatit (count, distinct, sum, avg, min, max)
- [x] 3.7 Validointivirheet koneluettavina (16 sääntöä, section-kohtaiset viestit)
- [x] 3.8 Deprecated normalisointi: ei vielä (uusi schema on ainoa tuettu)
- [ ] 3.9 Päivitä `glasspad docs spec` vastaamaan uutta schemaa
- [x] 3.10 Analytics-esimerkki päivitetty kanoniseen muotoon

## Vaihe 4: Turvallisuus ✅

- [x] 4.1 Pad-token (32 hex) generoidaan luontihetkellä
- [x] 4.2 Mutaatio-endpointit vaativat `X-Glasspad-Token`
- [x] 4.3 Token palautetaan CLI:n stdout-JSONissa (agentti käyttää)
- [x] 4.4 Pitkät pad-ID:t (UUID v4, 32 hex)
- [x] 4.5 CSP-headerit GET /:id -vastaukseen (+ frame-ancestors, base-uri, form-action)
- [ ] 4.6 `body_format: sanitized_html` allowlist-sanitoinnilla (text toimii)
- [x] 4.7 JSON-upotus: `<script type="application/json">` + `\u003c` escape
- [x] 4.8 Vega-specit safe_json_script_tag:lla (ei XSS inline scriptissä)

## Vaihe 5: Data layer ✅

- [x] 5.1 CSV-parser → `Vec<Row>` tyyppipäättelyllä
- [x] 5.2 JSON-parser → `Vec<Row>` (tyyppi säilyy, ei re-inferenssiä)
- [x] 5.3 Tyyppipäättely: numerot, booleanit, temporal, null (ei trimmausta)
- [x] 5.4 Dataset-metadata (FieldKind per sarake)
- [x] 5.5 Kokorajoitukset (50k riviä, 100 saraketta, 1MB solu, duplikaatti/tyhjä header → virhe)
- [x] 5.6 CLI `--data events=file.csv` (parsii, injektoi, case-insensitive extension)
- [ ] 5.7 API: multipart upload (nyt CLI injektoi inline, serveri tukee top-level+inline)
- [x] 5.8 `source:` resoluutio: collect_datasets tukee top-level + inline, ristiriitadetektointi
- [x] 5.9 Inline_data toimii ilman --data
- [x] 5.10 Arc<Pad> storessa (ei kloonausta)
- [x] 5.11 CLI: JSON stdout (id, url, token, title)
- [x] 5.12 ensure_server tarkistaa is_success()
- [x] 5.13 Stats-aggregaatiot rendererissä (count, distinct, sum, avg, min, max, where)

---

## Vaihe 6: Client-side renderöinti ⬜

> Nykyinen renderöinti on server-side (Rust generoi HTML:n).
> Client-side renderöinti mahdollistaa interaktiivisen suodatuksen (vaihe 7).

- [ ] 6.1 Datasets JSON selaimeen (application/json script tag per dataset)
- [ ] 6.2 JS-moduuli: parsii datasets, renderöi sectionit
- [ ] 6.3 Chart-renderöinti client-sidessa (Vega-Lite, datasta)
- [ ] 6.4 Table-renderöinti client-sidessa
- [ ] 6.5 Stats-aggregaatiot client-sidessa
- [ ] 6.6 Serveri generoi HTML-rungon + spec + data, JS tekee renderöinnin

## Vaihe 7: Interaktiivinen suodatus ⬜

> Ref: `04-spec-contract.md` §6, `06-arch-interactive-filtering.md`

- [ ] 7.1 Filter state -malli (per dataset, per field, Set<arvo>)
- [ ] 7.2 Chart-klikkaus → toggle filter (interactive_filter.field)
- [ ] 7.3 Suodatetun datan syöttö kaikkiin saman source:n sectioneihin
- [ ] 7.4 Filter bar (kelluva, tagit, reset-nappi)
- [ ] 7.5 Pulse-animaatio kun suodatus muuttuu
- [ ] 7.6 Section-tilan säilyminen
- [ ] 7.7 Testaus analytics-esimerkkidatalla

## Vaihe 8: Rikkaat datanäkymät (list) ⬜                  ← rinnastettavissa vaiheen 7 kanssa

> Ref: `04-spec-contract.md` §2 (list), `07-arch-rich-data-views.md`

- [ ] 8.1 List-section renderöinti (cards, rows, compact)
- [ ] 8.2 `id_field` pakollinen, validointi
- [ ] 8.3 Detail-näkymä (replace-moodi)
- [ ] 8.4 `body_format: text` renderöinti
- [ ] 8.5 `body_format: sanitized_html` renderöinti
- [ ] 8.6 Detail → back-navigaatio
- [ ] 8.7 List reagoi suodatuksiin

## Vaihe 9: Kaksisuuntaiset toiminnot ⬜

> Ref: `04-spec-contract.md` §7, `08-arch-bidirectional-actions.md`

- [ ] 9.1 Completion-endpoint: `POST /api/pads/:id/complete`
- [ ] 9.2 `GET /api/pads/:id/completion` (CLI pollaa)
- [ ] 9.3 Action-painikkeet detail-näkymässä
- [ ] 9.4 `row_actions` taulukossa
- [ ] 9.5 Done-painike + Cancel-painike
- [ ] 9.6 Pending actions JS-tilassa
- [ ] 9.7 Visuaalinen palaute
- [ ] 9.8 `--wait` CLI-lippu (blocking, timeout)
- [ ] 9.9 Pad lukitaan completionin jälkeen (409)

## Vaihe 10: Viimeistely ⬜

- [ ] 10.1 PID-tiedosto `~/.glasspad/server.pid`
- [ ] 10.2 `glasspad stop` -komento
- [ ] 10.3 `GLASSPAD_PORT` ympäristömuuttuja
- [ ] 10.4 `glasspad docs` päivitys uudella schemalla
- [ ] 10.5 Skill-päivitys
- [ ] 10.6 README päivitys
- [ ] 10.7 `cargo install` ja testaus toisessa repossa

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
Vaiheet 3–5: ✅ Tehty
    │
Vaihe 6: Client-side renderöinti
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
