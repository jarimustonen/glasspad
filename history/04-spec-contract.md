# Spec Contract: Glasspadin kanoninen sopimus

Tämä dokumentti määrittelee glasspadin YAML-specin, tietomallin, turvallisuusmallin,
CLI-sopimuksen ja completion-protokollan. Kaikki toteutus noudattaa tätä sopimusta.

Motivaatio: architecture review (review-architecture-v1.md) tunnisti schema-epävakaudet,
turvallisuusaukot ja protokolla-alimäärittelyt kriittisinä ongelmina.

---

## 1. YAML-spec schema

### Top-level

```yaml
spec_version: 1                          # pakollinen, kokonaisluku
title: "Dashboard title"                 # pakollinen
description: "Optional subtitle"         # valinnainen
layout: grid-2col                        # valinnainen, oletus: grid-2col
                                         # arvot: grid-2col, grid-3col, stack

datasets:                                # valinnainen, nimetyt datalähteet
  events: {}                             # tyhjä = CLI/API täyttää
  users: {}

sections:                                # pakollinen, lista sectioneja
  - id: by-country                       # pakollinen interaktiivisille, suositeltu kaikille
    title: "By country"                   # pakollinen
    type: chart                           # pakollinen: chart, table, stats, list
    source: events                        # viittaus datasets-lohkoon
    ...
```

### Nimeäminen: `datasets:` (ei `data:`)

Top-level avainsana on `datasets:`, ei `data:`. Tämä poistaa sekaannuksen
section-tason `inline_data:`-kentän kanssa.

### Datan sitominen sectioniin

Kaksi tapaa, **molemmat eivät voi olla läsnä samassa sectionissa**:

| Tapa | Kenttä | Käyttö |
|------|--------|--------|
| Viittaus | `source: events` | Section käyttää nimettyä datasettiä |
| Inline | `inline_data: [{...}]` | Data suoraan specissä (pienille staattisille dataseteille) |

`chart.data` ja `section.data` ovat **deprecated aliaksia** `inline_data`:lle.
Parser normalisoi ne, mutta uudet specit käyttävät aina `inline_data:` tai `source:`.

---

## 2. Section-tyypit

### chart

```yaml
- id: commits-per-day
  title: "Commits per day"
  type: chart
  source: commits                        # tai inline_data: [...]
  interactive_filter:                     # valinnainen
    field: author                         # mikä kenttä suodattuu klikkauksella
  chart:
    mark: bar                            # bar, line, arc
    encoding:
      x: { field: date, type: temporal }
      y: { aggregate: count, type: quantitative }
```

`interactive: true` + `filter_field: x` on **deprecated**. Kanoninen muoto on
`interactive_filter: { field: x }`.

Validointisääntö: `interactive_filter.field` pitää esiintyä jossakin
ei-aggregoidussa encoding-kanavassa.

### table

```yaml
- id: all-events
  title: "All events"
  type: table
  source: events
  table:
    columns:
      - { field: datetime, title: "Time", width: 120 }
      - { field: path, title: "Page" }
    row_id_field: id                     # pakollinen jos row_actions
    row_actions:
      - { id: approve, label: "✓", style: success }
```

### stats

```yaml
- id: summary
  title: "Summary"
  type: stats
  source: events
  stats:
    items:
      - { label: "Total events", aggregate: count }
      - { label: "Visits", aggregate: count, where: { event_type: visit } }
      - { label: "Countries", aggregate: distinct, field: country }
      - { label: "Avg score", aggregate: avg, field: score }
```

Tuetut aggregaatit: `count`, `distinct`, `sum`, `avg`, `min`, `max`.

`where:` suodattaa ennen aggregaatiota (field: value -yhtäsuuruus).

### list

```yaml
- id: inbox
  title: "Inbox"
  type: list
  source: emails
  list:
    id_field: id                         # pakollinen
    layout: cards                        # cards, rows, compact
    title_field: subject
    subtitle_field: from
    meta_field: date
    preview_field: body_preview
    item_click: detail                   # detail (oletus), filter
    detail:
      fields:
        - { field: from, title: "From" }
        - { field: date, title: "Date" }
      body_field: body
      body_format: text                  # text (oletus), sanitized_html
      actions:
        - { id: archive, label: "Archive", style: secondary }
        - { id: delete, label: "Delete", style: danger }
    on_action: fade                      # fade, hide, badge, none
  selectable: true                       # checkboxit batch-toiminnoille
  batch_actions:
    - { id: archive_all, label: "Archive selected" }
```

**Detail-moodi:** Vain `replace` MVP:ssä. Overlay ja fullscreen myöhemmin.

**`body_format`:** Oletus on `text` (turvallinen). `sanitized_html` ajaa
sisällön sanitoijan läpi (server-side allowlist). Raakaa HTML:ää ei koskaan renderöidä.

---

## 3. Tietomalli

### Dataset

```rust
type Row = BTreeMap<String, CellValue>;
type Dataset = Vec<Row>;

enum CellValue {
    Null,
    String(String),
    Number(f64),
    Bool(bool),
}
```

CSV-parser tuottaa `String`-arvoja. Tyyppipäättely normalisoi:
- ISO-8601 -merkkijonot → säilyvät `String`-tyyppisinä (temporal-metadata)
- Kokonaisluvut ja desimaalit → `Number(f64)`
- `true`/`false` (case-insensitive) → `Bool`
- Tyhjä kenttä → `Null`
- Muut → `String`

### Dataset-metadata

```rust
struct DatasetMeta {
    fields: BTreeMap<String, FieldKind>,
}

enum FieldKind {
    String,
    Number,
    Bool,
    Temporal,
}
```

Metadata päätellään automaattisesti ensimmäisestä 100 rivistä.

### Kokorajoitukset (MVP)

| Rajoitus | Arvo |
|----------|------|
| Rivejä per dataset | 50 000 |
| Datasetit per pad | 10 |
| Payload (JSON) | 20 MB |
| CSV-tiedoston koko | 50 MB |
| Sarakkeet per rivi | 100 |

Rajoitukset ylittävä syöte hylätään selkeällä virheilmoituksella.

---

## 4. Datan lataus

### CLI

```bash
# Datasets täytetään CLI-lipuilla
glasspad create --file dashboard.yaml --data events=events.csv --data users=users.json

# Inline data specissä (ei --data lippua tarvita)
glasspad create --file simple.yaml
```

CLI tunnistaa formaatin tiedostopäätteestä: `.csv` → CSV, `.json` → JSON.

### API

```
POST /api/pads
Content-Type: multipart/form-data

Part "spec": YAML-tiedosto
Part "dataset-events": CSV/JSON-tiedosto
Part "dataset-users": CSV/JSON-tiedosto
```

Yksinkertainen vaihtoehto (inline data):
```
POST /api/pads
Content-Type: application/x-yaml

(YAML jossa inline_data)
```

### Turvallisuussäännöt

- **Ei `file:` specissä.** Spec ei voi viitata tiedostopolkuihin. Tiedostot tulevat
  aina CLI `--data` -lipulla tai API multipart -uploadina.
- **Ei `url:` specissä.** Poistettu kunnes on erillinen turvallisuussuunnitelma.
- Spec-lohkossa `datasets: { events: {} }` tarkoittaa "tämä dataset täytetään ulkopuolelta".
- Jos spec viittaa `source: events` mutta datasettiä ei ole ladattu → validointivirhe.

---

## 5. JSON-upotus HTML:ään

Datasettejä **ei upoteta `<script>`-tägiin suoritettavana JS:nä**.

Kaksi turvallista vaihtoehtoa:

### A) Ei-suoritettava script-tagi (MVP)

```html
<script id="glasspad-data" type="application/json">
{"events":[...],"users":[...]}
</script>
<script>
  const datasets = JSON.parse(
    document.getElementById('glasspad-data').textContent
  );
</script>
```

Serveri escapaa `</script>` → `<\/script>` generoinnissa.

### B) Fetch-endpoint (isot datasetit, myöhemmin)

```
GET /api/pads/:id/data/:dataset_name → JSON array
```

Selain lataa datan asynkronisesti. Parempi isoille dataseteille.

---

## 6. Filter state -malli

### Rajaus datasettiin

```javascript
const filterState = {
  // source → field → Set<arvo>
  "events": {
    "country": new Set(["IN", "US"]),
    "device": new Set(["mobile"])
  }
};
```

### Boolean-logiikka

- Saman kentän sisällä: **OR** (country = IN tai US)
- Kenttien välillä: **AND** (country = IN|US JA device = mobile)
- Null-arvot eivät ole valittavissa interaktiivisesti
- Vertailu: strict equality normalisoidun arvon kanssa
- Case-sensitive

### Suodatuksen vaikutus

Suodatus vaikuttaa kaikkiin sectioneihin joilla on sama `source`.
Sectionit eri `source`:lla eivät vaikuta toisiinsa.

### Chartin klikkaus → suodatus

- Vain sectionit joilla on `interactive_filter.field` reagoivat klikkauksiin
- Klikkaus togglettaa arvon: lisää tai poistaa Set:istä
- Renderöinti: app-tason filter state on kanoninen, kaikki sectionit
  saavat suodatetun datan ja renderöidään uudelleen
- Charttien skaala-hyppy on hyväksyttävä MVP-tradeoff

### Section-tilan säilyminen

Suodatuksen muuttuessa:
- Chart: renderöidään uudelleen (skaala voi muuttua)
- Table: renderöidään uudelleen, scroll-positio nollaantuu
- Stats: lasketaan uudelleen
- List (lista-näkymä): renderöidään uudelleen
- List (detail-näkymä auki): jos kohde läpäisee suodatuksen → pysyy auki.
  Jos ei → palaa listanäkymään automaattisesti.

---

## 7. Completion-protokolla (`--wait`)

### Flow

```
CLI: glasspad create --file inbox.yaml --data emails=msgs.json --wait --json
  1. Luo padin (POST /api/pads)
  2. stderr: "Created pad abc123"
  3. stderr: "http://localhost:3000/abc123?token=SECRET"
  4. stderr: "Waiting for user... (timeout: 10m)"
  5. Pollaa: GET /api/pads/abc123/completion?token=SECRET
     (odottaa kunnes 200 tai timeout)
  6. stdout: {"status":"completed","pad_id":"abc123","actions":[...]}
     tai:    {"status":"cancelled","pad_id":"abc123","actions":[]}
     tai:    {"status":"timeout","pad_id":"abc123","actions":[]}
```

### Timeout

- `--timeout 5m` (oletus: 10m, 0 = ei timeoutia)
- Timeout → exit code 124, stdout: `{"status":"timeout",...}`
- Ctrl-C → exit code 130, stdout: `{"status":"cancelled",...}`

### Selainpuolen painikkeet

Kun padissa on toimintoja, renderöidään kelluvat painikkeet:

```
[ Cancel ]                    [ Done — send 4 actions ]
```

- **Done**: lähettää `POST /api/pads/:id/complete` finaalisen paketin
- **Cancel**: lähettää `POST /api/pads/:id/complete` tyhjällä actions-listalla + `status: cancelled`

### Completion-endpoint (atominen)

```
POST /api/pads/:id/complete
X-Glasspad-Token: SECRET
{
  "submission_id": "uuid",
  "actions": [
    { "action": "archive", "item_id": "msg-42", "item_summary": { "subject": "Re: Q1" } },
    { "action": "delete", "item_id": "msg-17", "item_summary": { "subject": "Standup" } }
  ]
}
```

- `submission_id` varmistaa idempotenttiuden (duplikaatti → sama vastaus)
- Pad merkitään completed, uudet eventit hylätään (409)
- Action-payload sisältää `item_id` + valinnaisen tiivistelmän, **ei koko datariviä**
- Kaikki toiminnot kerätään selaimessa JS-tilaan, lähetetään yhdellä kertaa Done-napista

### Event queue (optimistinen UI)

Toiminnot kerätään selaimessa JS-tilaan (ei POST joka klikkauksella):

```javascript
const pendingActions = [
  { action: "archive", item_id: "msg-42", item_summary: { subject: "..." } },
  ...
];
```

UI näyttää toiminnot optimistisesti (fade, badge). Done-nappi lähettää
koko `pendingActions`-arrayn yhtenä `POST /complete` -kutsuna.

Tämä poistaa tarpeen erilliselle event queuelle, duplikaattiongelmalle ja
race conditioneille.

### CLI-output-sopimus

| Kanava | Sisältö |
|--------|---------|
| stdout | Vain koneluettava data: pad-URL (create), JSON (--json/--wait), lista (list) |
| stderr | Statusviestit, varoitukset, virheet |

`--json` -lippu pakottaa JSON-muotoisen outputin myös create-komennolle:
```bash
glasspad create --file spec.yaml --json
# stdout: {"id":"abc123","url":"http://...","title":"..."}
```

---

## 8. Turvallisuusmalli

### Pad-token

Jokainen pad saa luontihetkellä kryptografisesti satunnaisen tokenin (32 hex).
Token vaaditaan kaikissa mutaatio-endpointeissa:

- `POST /api/pads/:id/events` → `X-Glasspad-Token: TOKEN`
- `POST /api/pads/:id/complete` → `X-Glasspad-Token: TOKEN`
- `PUT /api/pads/:id` → `X-Glasspad-Token: TOKEN`
- `DELETE /api/pads/:id` → `X-Glasspad-Token: TOKEN`

Token upotetaan renderöityyn HTML-sivuun (JS-muuttuja) joten selain osaa
lähettää sen. Ulkopuoliset sivut eivät tiedä tokenia.

Read-endpointit (GET) eivät vaadi tokenia.

### HTML-sisältö

- `body_format: text` (oletus): renderöidään `textContent`:ina, ei HTML:nä
- `body_format: sanitized_html`: server-side sanitointi ennen upotusta
  (allowlist: `p, br, strong, em, a, ul, ol, li, h1-h6, blockquote, pre, code`)
- Raaka HTML ei koskaan renderöidä body-kentistä

### CSP-headerit

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self' https://cdn.jsdelivr.net;
  style-src 'self' 'unsafe-inline';
  connect-src 'self';
  img-src 'self' data:;
```

### Pad-ID:t

UUID v4, täysi 32 hex (ei lyhennettyjä). Arvaamattomat.

---

## 9. Validointi ja virheet

### Spec-validointi (CLI/API)

Spec validoidaan ennen padin luontia. Virhe → exit code 1, stderr:iin
koneluettava virheilmoitus:

```
Error: spec validation failed
  - section[0] "by-country": interactive_filter.field "country" not found in chart encoding
  - section[2] "inbox": list requires id_field
  - section[3] "summary": unknown aggregate "median"
  - datasets.events: referenced by section "by-country" but not provided (use --data events=file.csv)
```

### Validointisäännöt

- `spec_version` on pakollinen ja tuettu
- `sections` on pakollinen ja ei-tyhjä
- `source` viittaa olemassa olevaan datasettiin
- `inline_data` ja `source` eivät voi molemmat olla läsnä
- `interactive_filter.field` esiintyy chartin encodingissa
- `list.id_field` on pakollinen jos listalla on `actions` tai `selectable`
- `table.row_id_field` on pakollinen jos taulukolla on `row_actions`
- Tuntematon section `type` → virhe
- Tuntematon `aggregate` → virhe
- Tuntematon `chart.mark` → virhe

---

## 10. Update-semantiikka

```bash
glasspad update abc123 --data events=new-events.csv
```

- **Read-only pad** (ei toimintoja): suodatukset nollataan jos dataset-schema muuttuu,
  muuten säilytetään. Section-tila (detail view) nollataan.
- **Actionable pad** (ei vielä completed): update sallittu, pending actions nollataan,
  käyttäjä saa varoituksen selaimessa.
- **Completed pad**: update hylätään (409 Completed). Luo uusi pad.

---

## 11. Serverin elinkaarihallinta

### Auto-start

`glasspad create/list/open` käynnistää serverin automaattisesti jos se ei ole
käynnissä. Serveri kirjoittaa PID-tiedoston `~/.glasspad/server.pid`.

### Prosessinhallinta

- `glasspad serve` → foreground, kirjoittaa PID-tiedoston
- Auto-start → background, kirjoittaa PID-tiedoston
- `glasspad stop` → lähettää SIGTERM PID-tiedoston prosessille
- PID-tiedoston olemassaolo + prosessin elinvoimaisuus tarkistetaan ennen auto-startia

### Portti

Oletus 3000. Konfiguroitava `--port` tai `GLASSPAD_PORT` ympäristömuuttujalla.

---

## 12. Deprecated-kentät (taaksepäin yhteensopivuus)

| Deprecated | Kanoninen | Normalisointi |
|-----------|-----------|---------------|
| top-level `data:` | `datasets:` | Parser hyväksyy molemmat, varoittaa |
| `section.data:` | `inline_data:` | Parser normalisoi |
| `chart.data:` | `inline_data:` (section-tasolla) | Parser siirtää ylös |
| `interactive: true` + `filter_field: x` | `interactive_filter: { field: x }` | Parser normalisoi |
| `datasets.x.file: "path"` | Ei tuettu specissä | Parser hylkää virheellä |

---

## Muutoshistoria

- v1 (2026-04-06): Ensimmäinen versio, pohjautuu architecture review -löydöksiin
