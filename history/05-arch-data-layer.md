# Arkkitehtuuri: Data Layer — ulkoiset datalähteet

## Ongelma

Nykyinen malli: data on inline YAML-specissä. Tämä ei skaalaudu kun:
- Data tulee CSV-tiedostosta (tuhansia rivejä)
- Sama data pitää näyttää eri näkymissä (chart + table + stats)
- Suodatukset muuttavat kaikkien näkymien dataa samanaikaisesti

## Ratkaisu: spec + data erikseen

```
Nykyinen:   YAML-spec (sisältää datan)  →  serveri  →  HTML
Uusi:       YAML-spec (viittaa dataan)  +  data     →  serveri  →  HTML + JS
```

Agentti lähettää kaksi asiaa:
1. **Spec** — kuvaa dashboardin rakenteen ja miten data esitetään
2. **Data** — CSV, JSON array, tai inline

## Tiedonsiirtoformaatti

### Spec viittaa dataan nimellä

```yaml
title: "3dbear.io — Analytics"
layout: grid-2col

data:
  events: { file: "analytics-events.csv" }       # ulkoinen tiedosto
  # tai
  events: { inline: [{...}, {...}] }              # inline JSON
  # tai
  events: { url: "http://localhost:8080/data.csv" }  # URL (myöhemmin)

sections:
  - title: "Visits per country"
    type: chart
    source: events                                 # viittaa data-lohkoon
    chart:
      mark: bar
      encoding:
        x: { field: country, type: nominal }
        y: { aggregate: count, type: quantitative }

  - title: "All events"
    type: table
    source: events
    columns:
      - { field: datetime, title: "Time" }
      - { field: path, title: "Page" }
      - { field: country, title: "Country" }
      - { field: device, title: "Device" }
      - { field: event_type, title: "Type" }
```

### Datan lataus CLI:ssä

```bash
# Spec + data-tiedosto samalla kertaa
glasspad create --file dashboard.yaml --data events=analytics-events.csv

# Useita datalähteitä
glasspad create --file dashboard.yaml \
  --data events=events.csv \
  --data users=users.json

# Inline data (nykyinen malli, toimii edelleen)
glasspad create --file dashboard-with-inline-data.yaml
```

### API

```
POST /api/pads
Content-Type: multipart/form-data

spec: (YAML-tiedosto)
data-events: (CSV-tiedosto)
```

tai yksinkertaisemmin:

```
POST /api/pads
Content-Type: application/x-yaml

(YAML jossa inline data)
```

## Arkkitehtuuri serverissä

```
┌─────────────────────────────────────────────────────┐
│                    Pad Storage                       │
│                                                      │
│   spec (DashboardSpec)                               │
│   datasets: HashMap<String, Dataset>                 │
│     "events" → Vec<Row>  (parsittu CSV/JSON)         │
│                                                      │
└─────────────────────────────────────────────────────┘
```

**Dataset** on Vec<serde_json::Value> — rivejä joissa kenttä-arvopareja.
CSV parsitaan rivit-objekteiksi ladattaessa.

## Renderöintimuutos

Nykyinen: serveri generoi HTML:n, data on staattisesti upotettuna.
Uusi: serveri generoi HTML:n + **upottaa datan JSON-muodossa** `<script>`-tägiin.

```html
<script>
  const datasets = {
    "events": [
      {"datetime":"2026-04-04T18:00:00Z","path":"/en/","country":"OM",...},
      ...
    ]
  };
</script>
```

Vega-Lite specit viittaavat dataan: `"data": {"values": datasets["events"]}`.

Tämä mahdollistaa myöhemmin client-side suodatuksen ilman serveri-roundtrippia.

## Taaksepäin yhteensopivuus

Inline data toimii edelleen:
```yaml
sections:
  - title: "Chart"
    type: chart
    chart:
      data: [{x: 1, y: 2}]         # inline, kuten ennenkin
```

Jos `source` puuttuu ja `chart.data` on annettu, käytetään inline-dataa.
Jos `source` on annettu, haetaan data `datasets`-lohkosta.
