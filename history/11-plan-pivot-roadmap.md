# Plan: Pivot-taulukko roadmap

## Arkkitehtuuridokumentit

| # | Dokumentti | Sisältö |
|---|-----------|---------|
| 05 | arch-data-layer | Ulkoiset datalähteet (CSV/JSON), spec+data erikseen |
| 06 | arch-interactive-filtering | Klikkaussuodatus, filter bar, advanced filters |
| 07 | arch-rich-data-views | List-section, detail-näkymä, card/row/compact |
| 08 | arch-bidirectional-actions | Toimintopainikkeet, event queue, batch-toiminnot |
| 09 | ref-cli-examples | CLI-esimerkit kaikille vaiheille |
| 10 | ref-data-format | Tiedonsiirtoformaatti: spec, data, events |

## Toteutusvaiheet

```
Vaihe A: Data layer                    ← perusta kaikelle
    │
    ├── A1: CSV/JSON parser (serveri)
    ├── A2: data-lohko YAML-specissä (source-viittaukset)
    ├── A3: --data CLI-lippu
    ├── A4: Multipart API upload
    ├── A5: Data upotetaan HTML:ään JSON-muodossa
    │
Vaihe B: Client-side renderöinti       ← siirto staattisesta dynaamiseen
    │
    ├── B1: JS-moduuli: data loading + section rendering
    ├── B2: Vega-Lite chartit renderöidään client-sidessa datasta
    ├── B3: Taulukko renderöidään client-sidessa
    ├── B4: Stats aggregoidaan client-sidessa
    │
Vaihe C: Interaktiivinen suodatus      ← pivot-taulukko ydin
    │
    ├── C1: Filter state management (JS)
    ├── C2: Chart-klikkaus → suodatus
    ├── C3: Filter bar (kelluva, tagit, reset)
    ├── C4: Kaikkien sectionien uudelleenrenderöinti suodatuksella
    ├── C5: Pulse-animaatio filter barin muuttuessa
    ├── C6: Advanced filters -paneeli
    │
Vaihe D: Rikkaat datanäkymät           ← list-section
    │
    ├── D1: List-section renderöinti (cards/rows/compact)
    ├── D2: Detail-näkymä (klikkaus → yksittäinen kohde)
    ├── D3: List + suodatukset
    │
Vaihe E: Kaksisuuntaiset toiminnot     ← pad → agent (blocking-malli)
    │
    ├── E1: Event queue (serveri, in-memory)
    ├── E2: POST /api/pads/:id/events (selain → serveri)
    ├── E3: POST /api/pads/:id/done (selain ilmoittaa valmis)
    ├── E4: GET /api/pads/:id/done + GET events (CLI pollaa + lukee)
    ├── E5: --wait lippu: CLI blokkaa kunnes Done
    ├── E6: Done-painike UI:ssa (kelluva, näyttää toimintojen lkm)
    ├── E7: Action-painikkeet (detail, row_actions)
    ├── E8: Batch-toiminnot + checkbox-valinta
    │
Vaihe F: Dokumentaation päivitys
    │
    ├── F1: glasspad docs päivitys (source, interactive, list, actions, --wait)
    ├── F2: Skill-päivitys
    ├── F3: Esimerkkipadit
    │
Vaihe G: OpenClaw-integraatio (tulevaisuus)
    │
    ├── G1: Glasspad OpenClaw-päätelaitteena (oma sessio)
    ├── G2: Toiminnot kontekstina seuraavaan turniin (ei pollausta)
    ├── G3: Pitkäikäinen sessio padien välillä
```

## Rinnakkaisuus

```
Vaihe A: Data layer
    │
Vaihe B: Client-side renderöinti
    │
    ├──────────────────┐
    │                  │
Vaihe C: Suodatus   Vaihe D: List     ← RINNAKKAIN (molemmat tarvitsevat B:n)
    │                  │
    ├──────────────────┘
    │
Vaihe E: Toiminnot / --wait (tarvitsee C + D)
    │
Vaihe F: Dokumentaatio
    │
Vaihe G: OpenClaw-integraatio (tulevaisuus, erillinen suunnittelu)
```

## Ensimmäinen konkreettinen tavoite

Analytics-dashboard (history/examples/analytics-dashboard.yaml) toimii
oikealla datalla (history/analytics-*-events.csv) ja käyttäjä voi:

1. Nähdä chartit ja taulukon
2. Klikata country-charttia → kaikki sectionit suodattuvat
3. Klikata device-charttia → lisäsuodatus
4. Nollata suodatukset filter barista

Tämä vaatii vaiheet A + B + C.

## Esimerkkidatan käyttö

```bash
glasspad create \
  --file history/examples/analytics-dashboard.yaml \
  --data events=history/analytics-3dbear-io-24h-2026-04-05-events.csv
```
