# Glasspad — Työsuunnitelma

AI scratchpad for rich data views.

## Status

🚧 Tutkimusvaihe — Vaihe 1 käynnissä

---

## Vaihe 1: Tutkimus ✅ osittain

- [x] 1.1 Markkina- ja teknologiakatsaus → `history/01-research-landscape.md`
  - [x] a) Dashboard- ja visualisointikirjastot (open source)
  - [x] b) AI-visuaaliset tuotokset markkinoilla (Artifacts, Canvas, A2UI, Open WebUI)
  - [x] c) Ominaisuuskartoitus glasspadia varten
- [x] 1.2 Integraatiomalli tekoälyjen kanssa → `history/02-design-integration-model.md`
  - [x] Konseptitason API (agent → pad → selain → agent)
  - [x] Callback-mekanismi (pad → agent kaksisuuntaisuus)
  - [x] Sisältötyypit ja turvallisuusmalli
- [ ] 1.3 Teknologiavalinnat ja perustelut → `history/03-design-tech-choices.md`

## Vaihe 2: Projektin runko ⬜

> Edellytys: Vaihe 1 valmis (teknologiavalinnat tehty)

- [ ] 2.1 Alustetaan projekti (package.json, tsconfig, linter, formatter)
- [ ] 2.2 Perus dev-ympäristö (hot reload, testit)
- [ ] 2.3 CI-pohja (lint + test)

## Vaihe 3: Core API ⬜

> Edellytys: Vaihe 2 valmis

- [ ] 3.1 Storage-kerros (padin tallennus ja haku, SQLite)
- [ ] 3.2 POST /api/pads — sisällön luonti, palauttaa { id, url }
- [ ] 3.3 GET /api/pads/:id — padin metadata
- [ ] 3.4 PUT /api/pads/:id — padin päivitys
- [ ] 3.5 DELETE /api/pads/:id — padin poisto
- [ ] 3.6 Padien automaattinen vanheneminen (TTL + cleanup)

## Vaihe 4: Renderöinti ⬜                                    ← rinnastettavissa vaiheen 3 kanssa

> Edellytys: Vaihe 2 valmis (voi edetä rinnakkain vaiheen 3 kanssa)

- [ ] 4.1 GET /:id — padin renderöinti selaimessa
- [ ] 4.2 HTML-sisällön sandboxed rendering (iframe + CSP)
- [ ] 4.3 Chart-renderöinti (Vega-Lite JSON → SVG/Canvas)
- [ ] 4.4 Markdown-renderöinti
- [ ] 4.5 Taulukkonäkymä (JSON/CSV → interaktiivinen taulukko)
- [ ] 4.6 Responsiivinen layout

## Vaihe 5: Kaksisuuntaisuus (pad ↔ agent) ⬜

> Edellytys: Vaiheet 3 + 4 valmiit

- [ ] 5.1 SSE-stream: agentti → selain (live-päivitykset)
- [ ] 5.2 Event queue: selain → agentti (käyttäjäinteraktiot)
  - POST /api/pads/:id/events (selain lähettää)
  - GET /api/pads/:id/events?since= (agentti lukee)
- [ ] 5.3 Interaktiiviset kontrollit padissa (napit, sliderit, lomakkeet)
- [ ] 5.4 Dashboard-layout (useita padeja yhdessä näkymässä)

## Vaihe 6: MCP / CLI -integraatio ⬜                         ← rinnastettavissa vaiheen 5 kanssa

> Edellytys: Vaihe 3 valmis (voi edetä rinnakkain vaiheen 5 kanssa)

- [ ] 6.1 MCP-serveri: create_pad, update_pad, get_pad_events, list_pads, delete_pad
- [ ] 6.2 CLI-työkalu: glasspad create/update/list/events/delete
- [ ] 6.3 Testaus Claude Code -ympäristössä
- [ ] 6.4 OpenClaw-plugin tuki
- [ ] 6.5 Dokumentaatio (MCP-asennus, CLI-käyttö, esimerkit)

## Vaihe 7: Tuotantovalmius ⬜

> Edellytys: Vaiheet 5 + 6 valmiit

- [ ] 7.1 Rate limiting
- [ ] 7.2 Konfigurointi (env-muuttujat, portit, storage-polku)
- [ ] 7.3 Docker-image
- [ ] 7.4 README ja API-dokumentaatio
- [ ] 7.5 Esimerkkipadeja (demo-sisältö)

---

## Rinnakkaisuusanalyysi

```
Vaihe 1: Tutkimus
    │
Vaihe 2: Projektin runko
    │
    ├──────────────────────┐
    │                      │
Vaihe 3: Core API    Vaihe 4: Renderöinti     ← RINNAKKAIN
    │                      │
    ├──────────────────────┘
    │
    ├──────────────────────┐
    │                      │
Vaihe 5: Kaksisuunt.  Vaihe 6: MCP/CLI        ← RINNAKKAIN
    │                      │
    ├──────────────────────┘
    │
Vaihe 7: Tuotantovalmius
```

**Rinnakkaistyömahdollisuudet:**
1. **Vaiheet 3 + 4** — API ja renderöinti ovat itsenäisiä (molemmat tarvitsevat vain rungon)
2. **Vaiheet 5 + 6** — kaksisuuntaisuus ja MCP/CLI voivat edetä rinnakkain (MCP tarvitsee vain core APIn)
3. Worktree-malli: toinen agentti tekee renderöintiä, toinen API:a

---

## Avoimet kysymykset (ratkaistaan vaiheessa 1.3)

- Runtime: Node.js + Fastify vai Bun vai Deno?
- Storage: SQLite (paras) vai tiedostopohjainen vai muistissa?
- Chart-kirjasto: Vega-Lite (deklaratiivinen) vai Chart.js (yksinkertainen)?
- Vanheneminen: 24h oletus, konfiguroitava?
- MCP: samaan prosessiin vai erillinen?
- A2UI-yhteensopivuus: tuetaanko deklaratiivista komponenttimallia?
