# Arkkitehtuuri: Interaktiivinen suodatus

## Edellytys

Data layer (05-arch-data-layer.md) — data on erillään specistä ja ladattuna
selaimeen JSON-muodossa. Suodatus tapahtuu client-side.

## Konsepti

Dashboardin sectionit voivat olla **interaktiivisia suodattimia**. Kun käyttäjä
klikkaa chartissa palkkia tai taulukon riviä, se lisää suodatuksen joka
vaikuttaa kaikkiin saman datalähteen sectioneihin.

```
┌──────────────────────────────────────────────────────────────┐
│  [🔍 Filters: country=IN, device=mobile]           [Reset]  │  ← filter bar
│                                                              │
│  ┌─────────────────────┐  ┌───────────────────────────────┐  │
│  │ Visits per country  │  │ Device breakdown              │  │
│  │ ▐▐▐▐▐▐▐▐▐          │  │                               │  │
│  │ ▐▐▐▐▐▐▐ ← IN valittu│  │   ██ mobile ← valittu        │  │
│  │ ▐▐▐▐                │  │   ██ desktop                  │  │
│  │ ▐▐                  │  │   ██ tablet                   │  │
│  └─────────────────────┘  └───────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ All events (filtered: 42 / 228)                        │  │
│  │ datetime    path                country  device        │  │
│  │ 08:20       /blog/impact-of-ai  IN       mobile        │  │
│  │ ...                                                    │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

## YAML-spec

```yaml
sections:
  - title: "Visits per country"
    type: chart
    source: events
    interactive: true                    # ← tämä kenttä aktivoi suodatuksen
    filter_field: country                # klikkaus suodattaa tällä kentällä
    chart:
      mark: bar
      encoding:
        x: { field: country, type: nominal }
        y: { aggregate: count, type: quantitative }

  - title: "Device breakdown"
    type: chart
    source: events
    interactive: true
    filter_field: device
    chart:
      mark: arc
      encoding:
        theta: { aggregate: count, type: quantitative }
        color: { field: device, type: nominal }

  - title: "All events"
    type: table
    source: events
    # taulukko reagoi suodatuksiin mutta ei itse suodata
    columns:
      - { field: datetime, title: "Time" }
      - { field: path, title: "Page" }
      - { field: country, title: "Country" }
      - { field: device, title: "Device" }
```

## Client-side arkkitehtuuri

### Filter state

```javascript
// Globaali suodatustila
const filterState = {
  // field → Set<arvo>
  // esim: { "country": Set(["IN"]), "device": Set(["mobile"]) }
};
```

### Suodatuslogiikka

```
1. Käyttäjä klikkaa chart-palkkia (country=IN)
2. → filterState.country = Set(["IN"])  (toggle: klikkaa uudelleen poistaa)
3. → kaikki sectionit, joilla source=events, renderöidään uudelleen
     suodatetulla datalla
4. → filter bar päivittyy näyttämään aktiiviset suodatukset
5. → filter bar animoituu (pulse) kiinnittämään huomion
```

### Aggregaatio client-sidessa

Chartit joissa `aggregate: count` eivät voi käyttää Vega-Liten omaa aggregaatiota
suoraan, koska suodatus muuttaa dataa. Kaksi vaihtoehtoa:

**A) Vega-Lite hoitaa** — syötetään suodatettu data Vega-Litelle, sen oma
`aggregate` toimii. Yksinkertaisempi.

**B) Itse aggregoidaan** — lasketaan aggregaatti JS:ssä ennen Vega-Litelle
syöttämistä. Tarvitaan jos halutaan näyttää "valitut vs. muut" eri väreillä.

→ Aloitetaan vaihtoehdolla A. Vega-Lite saa aina suodatetun raakadatan.

### Filter bar

Kelluva palkki dashboardin yläreunassa.

- **Oletustila**: ohut, huomaamaton, teksti "No filters"
- **Kun suodatuksia**: laajenee, näyttää tagit (esim. `country: IN ×`)
- **Pulse-animaatio**: kun suodatus lisätään, filter bar välähtää lyhyesti
- **Reset-nappi**: poistaa kaikki suodatukset kerralla
- **Tagien poisto**: yksittäinen suodatus poistettavissa ×-napilla

### Edistynyt suodatuspaneeli

Filter barin klikkaus avaa laajemman paneelin:

```
┌─────────────────────────────────────────┐
│  Advanced Filters                    ×  │
│                                         │
│  country:  [IN] [US] [DE] [AU] ...     │  ← multi-select chips
│  device:   [mobile] [desktop] [tablet]  │
│  browser:  [Chrome] [Safari] [Edge]     │
│  path:     [ contains... ]              │  ← text filter
│  datetime: [2026-04-04] → [2026-04-05]  │  ← range
│                                         │
│  [Apply]  [Reset all]                   │
└─────────────────────────────────────────┘
```

Tämä on vaihe 2 — ensin perus-klikkaussuodatus, sitten paneeli.

## Vaikutus serverin arkkitehtuuriin

Minimaaliset muutokset:
- Uudet kentät specissä: `interactive`, `filter_field`
- Renderöinnissä: generoitu JS sisältää suodatuslogiikan
- **Ei serveri-roundtrippia** — kaikki suodatus tapahtuu selaimessa

Serveri generoi yhden HTML-sivun jossa on:
1. Data JSON-muodossa
2. Dashboard-rakenne
3. Suodatus-JS-koodi (generoitu tai staattinen kirjasto)
