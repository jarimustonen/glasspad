# Referenssi: Tiedonsiirtoformaatti

## Yleiskuva

Glasspadiin lähetetään kaksi asiaa: **spec** ja **data**.

```
Spec (YAML)     — mitä näytetään, miten, missä järjestyksessä
Data (CSV/JSON)  — rivipohjainen data johon spec viittaa
```

## Spec-formaatti (YAML)

### Rakenne

```yaml
title: string                              # dashboardin otsikko
description: string                        # valinnainen
layout: grid-2col | grid-3col | stack      # valinnainen, oletus grid-2col

data:                                      # valinnainen, nimetyt datalähteet
  <nimi>: { file: "polku.csv" }            # ulkoinen tiedosto
  <nimi>: { inline: [{...}] }              # inline JSON

sections:                                  # lista sectioneja
  - title: string
    type: chart | table | stats | list
    source: string                         # viittaus data-lohkoon
    interactive: bool                      # suodatettavissa klikkaamalla
    filter_field: string                   # mikä kenttä suodattuu

    # type-kohtaiset kentät:
    chart: { ... }
    columns: [{ ... }]
    data: [{ ... }]                        # inline data (ilman source:a)
    list: { ... }
    actions: [{ ... }]
    row_actions: [{ ... }]
    selectable: bool
    batch_actions: [{ ... }]
```

### Esimerkkejä

Minimaalinen:
```yaml
title: "Test"
sections:
  - title: "OK"
    type: stats
    data:
      - { label: "Status", value: "OK" }
```

Ulkoisella datalla:
```yaml
title: "Analytics"
data:
  events: { file: "events.csv" }
sections:
  - title: "Visits"
    type: chart
    source: events
    chart:
      mark: bar
      encoding:
        x: { field: country, type: nominal }
        y: { aggregate: count, type: quantitative }
```

## Data-formaatit

### CSV

```csv
datetime,path,country,device,event_type
2026-04-04T18:00:00Z,/en/,OM,mobile,pageview
2026-04-04T18:00:00Z,/en/,OM,mobile,visit
```

- Ensimmäinen rivi on header
- Kentät erotetaan pilkulla
- UTF-8
- Glasspad parsii jokaisen rivin JSON-objektiksi: `{"datetime":"2026-04-04T18:00:00Z","path":"/en/",...}`

### JSON

```json
[
  {"datetime":"2026-04-04T18:00:00Z","path":"/en/","country":"OM"},
  {"datetime":"2026-04-04T18:00:00Z","path":"/en/","country":"IN"}
]
```

- Array of objects
- Jokainen objekti on yksi rivi

### Inline YAML

```yaml
data:
  items:
    inline:
      - { name: "Alice", score: 95 }
      - { name: "Bob", score: 87 }
```

## Datan lataus API:ssa

### Multipart (spec + ulkoiset tiedostot)

```
POST /api/pads
Content-Type: multipart/form-data

--boundary
Content-Disposition: form-data; name="spec"; filename="dashboard.yaml"
Content-Type: application/x-yaml

(YAML-spec)
--boundary
Content-Disposition: form-data; name="data-events"; filename="events.csv"
Content-Type: text/csv

(CSV-data)
--boundary--
```

### Yksinkertainen (inline data)

```
POST /api/pads
Content-Type: application/x-yaml

(YAML jossa data on inline tai ei viitata ulkoisiin tiedostoihin)
```

## Datan lataus CLI:ssä

```bash
# --data <nimi>=<polku> mapittaa nimen tiedostoon
glasspad create --file spec.yaml --data events=events.csv

# Tunnistaa formaatin tiedostopäätteestä:
#   .csv  → parsitaan CSV
#   .json → parsitaan JSON
#   .yaml → parsitaan YAML
```

## Event-formaatti (pad → agent)

Kun käyttäjä tekee toiminnon padissa, event tallennetaan:

```json
{
  "seq": 1,
  "type": "action",
  "action": "archive",
  "item": {
    "id": "msg-42",
    "subject": "Re: Q1 Budget",
    "from": "maria@example.com"
  },
  "timestamp": "2026-04-06T12:00:00Z"
}
```

| Kenttä | Tyyppi | Kuvaus |
|--------|--------|--------|
| seq | number | Juokseva numero, kasvaa monotonisesti |
| type | string | "action", "filter_change", "selection" |
| action | string | Toiminnon id (specissä määritelty) |
| item | object | Klikatun kohteen koko data-rivi |
| items | array | Batch-toiminnossa: kaikki valitut kohteet |
| timestamp | string | ISO 8601 |

## Tyyppijärjestelmä

Kenttien tyypit Vega-Lite -henkisesti:

| Tyyppi | Käyttö | Esimerkit |
|--------|--------|-----------|
| quantitative | Numerot | revenue, count, temperature |
| nominal | Kategoriat (ei järjestystä) | country, browser, device |
| ordinal | Kategoriat (järjestys) | month, priority, rating |
| temporal | Aikaleimat | datetime, created_at |

Glasspad ei pakota tyyppejä — ne ohjaavat renderöintiä (akselien muotoilu,
lajittelu, värikartat).
