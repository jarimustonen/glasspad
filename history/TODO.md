# Glasspad — Työsuunnitelma

AI scratchpad for rich data views.

## Status

PoC toimii (inline YAML → HTML). Seuraavaksi: spec contract → data layer → interaktiivisuus.

Sopimus: `history/04-spec-contract.md`
Review: `history/review-architecture-v1.md`

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

---

## Vaihe 3: Spec contract -toteutus ⬜

> Ref: `04-spec-contract.md` §1–2, §9, §12

Nykyinen parser hyväksyy löyhän YAML:n. Tässä vaiheessa
toteutetaan kanoninen schema ja validointi.

- [ ] 3.1 `spec_version: 1` pakolliseksi
- [ ] 3.2 `datasets:` top-level (normalisoi deprecated `data:`)
- [ ] 3.3 `inline_data:` section-tasolla (normalisoi `chart.data`, `section.data`)
- [ ] 3.4 `interactive_filter: { field: x }` (normalisoi `interactive` + `filter_field`)
- [ ] 3.5 Section `id:` -kenttä (pakollinen interaktiivisille)
- [ ] 3.6 Stats-schema: `stats.items` + aggregaatit (count, distinct, sum, avg, min, max)
- [ ] 3.7 Validointivirheet stderriin (koneluettavat, rivi/section-kohtaiset)
- [ ] 3.8 Deprecated-kenttien normalisointi + varoitukset
- [ ] 3.9 Päivitä `glasspad docs spec` vastaamaan uutta schemaa
- [ ] 3.10 Päivitä esimerkkitiedostot kanoniseen muotoon

## Vaihe 4: Turvallisuus ⬜

> Ref: `04-spec-contract.md` §5, §8

- [ ] 4.1 Pad-token (32 hex) generoidaan luontihetkellä
- [ ] 4.2 Mutaatio-endpointit vaativat `X-Glasspad-Token`
- [ ] 4.3 Token upotetaan renderöityyn HTML:ään
- [ ] 4.4 Pitkät pad-ID:t (UUID v4, 32 hex, ei lyhennettyjä)
- [ ] 4.5 CSP-headerit GET /:id -vastaukseen
- [ ] 4.6 `body_format: text` oletus, `sanitized_html` allowlist-sanitoinnilla
- [ ] 4.7 JSON-upotus: `<script type="application/json">` (ei suoritettava)
- [ ] 4.8 Spec-validointi: hylkää `file:` ja `url:` datasets-lohkossa

## Vaihe 5: Data layer ⬜

> Ref: `04-spec-contract.md` §3–4, `05-arch-data-layer.md`

- [ ] 5.1 CSV-parser → `Vec<Row>` (BTreeMap<String, CellValue>)
- [ ] 5.2 JSON-parser → `Vec<Row>`
- [ ] 5.3 Tyyppipäättely: numerot, booleanit, temporal-merkkijonot, null
- [ ] 5.4 Dataset-metadata (FieldKind per sarake)
- [ ] 5.5 Kokorajoitukset (50k riviä, 20MB payload, 50MB CSV)
- [ ] 5.6 CLI `--data events=file.csv` -lippu (multipart tai luku ennen lähetystä)
- [ ] 5.7 API: multipart upload (spec + dataset-tiedostot)
- [ ] 5.8 `source:` -viittauksen resoluutio: tarkista että dataset on ladattu
- [ ] 5.9 Taaksepäin yhteensopivuus: inline_data toimii edelleen ilman --data

## Vaihe 6: Client-side renderöinti ⬜                     ← rinnastettavissa vaiheen 5 kanssa

> Ref: `04-spec-contract.md` §5, `06-arch-interactive-filtering.md`

Nykyinen renderöinti on server-side (Rust generoi HTML:n).
Tässä vaiheessa selain renderöi sectionit datasta.

- [ ] 6.1 Datasets JSON selaimeen (application/json script tag)
- [ ] 6.2 JS-moduuli: parsii datasets, renderöi sectionit
- [ ] 6.3 Chart-renderöinti client-sidessa (Vega-Lite, datasta)
- [ ] 6.4 Table-renderöinti client-sidessa
- [ ] 6.5 Stats-aggregaatiot client-sidessa (count, distinct, sum, avg, min, max, where)
- [ ] 6.6 Serveri generoi HTML-rungon + upottaa datan + lataa JS:n

## Vaihe 7: Interaktiivinen suodatus ⬜

> Ref: `04-spec-contract.md` §6, `06-arch-interactive-filtering.md`

- [ ] 7.1 Filter state -malli (per dataset, per field, Set<arvo>)
- [ ] 7.2 Chart-klikkaus → toggle filter (interactive_filter.field)
- [ ] 7.3 Suodatetun datan syöttö kaikkiin saman source:n sectioneihin
- [ ] 7.4 Filter bar (kelluva, tagit, reset-nappi)
- [ ] 7.5 Pulse-animaatio kun suodatus muuttuu
- [ ] 7.6 Section-tilan säilyminen (detail view auki → sulkeutuu jos kohde suodatettu pois)
- [ ] 7.7 Testaus analytics-esimerkkidatalla

## Vaihe 8: Rikkaat datanäkymät (list) ⬜                  ← rinnastettavissa vaiheen 7 kanssa

> Ref: `04-spec-contract.md` §2 (list), `07-arch-rich-data-views.md`

- [ ] 8.1 List-section renderöinti (cards, rows, compact)
- [ ] 8.2 `id_field` pakollinen, validointi
- [ ] 8.3 Detail-näkymä (replace-moodi, klikkaus → yksittäinen kohde)
- [ ] 8.4 `body_format: text` renderöinti
- [ ] 8.5 `body_format: sanitized_html` renderöinti (allowlist-sanitointi)
- [ ] 8.6 Detail → back-navigaatio
- [ ] 8.7 List reagoi suodatuksiin (vaiheen 7 jälkeen)

## Vaihe 9: Kaksisuuntaiset toiminnot ⬜

> Ref: `04-spec-contract.md` §7, `08-arch-bidirectional-actions.md`

- [ ] 9.1 Completion-endpoint: `POST /api/pads/:id/complete` (atominen, idempot.)
- [ ] 9.2 `GET /api/pads/:id/completion` (CLI pollaa)
- [ ] 9.3 Action-painikkeet detail-näkymässä
- [ ] 9.4 `row_actions` taulukossa
- [ ] 9.5 Done-painike + Cancel-painike (kelluva)
- [ ] 9.6 Pending actions JS-tilassa, lähetetään yhdellä kertaa
- [ ] 9.7 Visuaalinen palaute (fade/hide/badge)
- [ ] 9.8 `--wait` CLI-lippu (pollaa completion, timeout, cancel)
- [ ] 9.9 `--timeout` lippu (oletus 10m)
- [ ] 9.10 `--json` lippu (stdout vain koneluettavaa)
- [ ] 9.11 Batch-toiminnot (`selectable: true`, `batch_actions`)
- [ ] 9.12 Pad lukitaan completionin jälkeen (409)

## Vaihe 10: Serverin elinkaarihallinta ⬜

> Ref: `04-spec-contract.md` §11

- [ ] 10.1 PID-tiedosto `~/.glasspad/server.pid`
- [ ] 10.2 Auto-start tarkistaa PID:n elinvoimaisuuden
- [ ] 10.3 `glasspad stop` -komento
- [ ] 10.4 `GLASSPAD_PORT` ympäristömuuttuja

## Vaihe 11: Dokumentaatio ja viimeistely ⬜

- [ ] 11.1 `glasspad docs` päivitys (datasets, source, interactive_filter, list, actions, --wait)
- [ ] 11.2 Skill-päivitys (uusi schema, --wait, --json)
- [ ] 11.3 README päivitys (asennus, quickstart, esimerkit)
- [ ] 11.4 Esimerkkipadit uudella schemalla

## Vaihe 12: MCP-integraatio ⬜

- [ ] 12.1 MCP-serveri: create_pad, update_pad, list_pads, delete_pad
- [ ] 12.2 MCP: wait_for_completion (blocking tool)
- [ ] 12.3 Testaus Claude Code -ympäristössä

## Tulevaisuus (ei aikataulua)

- [ ] OpenClaw-päätelaite (oma sessio, toiminnot turn-kontekstina) → `08 §Tulevaisuus`
- [ ] Fetch-endpoint isoille dataseteille (`GET /api/pads/:id/data/:name`)
- [ ] Advanced filters -paneeli (multi-select, range, text search)
- [ ] Detail-moodit: overlay, fullscreen
- [ ] A2UI-yhteensopivuus
- [ ] Docker-image
- [ ] SQLite-persistenssi (nyt in-memory)

---

## Rinnakkaisuusanalyysi

```
Vaihe 3: Spec contract
    │
    ├──────────────────────┐
    │                      │
Vaihe 4: Turvallisuus   Vaihe 5: Data layer      ← RINNAKKAIN
    │                      │
    ├──────────────────────┘
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
Vaihe 10–12: Viimeistely, MCP
```
