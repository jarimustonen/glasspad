# Arkkitehtuuri: Rikkaat datanäkymät

## Edellytys

Data layer (05) + interaktiivinen suodatus (06).

## Ongelma

Nykyiset section-tyypit (chart, table, stats) esittävät dataa tilastollisesti.
Mutta joskus data on **sisältöä** — sähköposteja, tikettejä, dokumentteja,
lokimerkintöjä — ja käyttäjä haluaa selata ja avata yksittäisiä kohteita.

## Uusi section-tyyppi: `list`

```yaml
sections:
  - title: "Inbox"
    type: list
    source: emails
    list:
      layout: cards                          # cards | rows | compact
      title_field: subject
      subtitle_field: from
      meta_field: date
      preview_field: body_preview
      detail:
        fields:
          - { field: from, title: "From" }
          - { field: to, title: "To" }
          - { field: date, title: "Date" }
          - { field: subject, title: "Subject" }
        body_field: body_html                # renderöidään HTML:nä
```

## Visuaalinen rakenne

### List-näkymä (cards)

```
┌──────────────────────────────────────────────────┐
│  Inbox (142 messages)                            │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │ Re: Q1 Budget Review              2h ago  │  │
│  │ Maria Chen <maria@example.com>             │  │
│  │ Thanks for the updated numbers. I've...    │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │ Deploy notification: v2.4.1        4h ago  │  │
│  │ deploy-bot@internal                        │  │
│  │ Production deploy completed successfully...│  │
│  └────────────────────────────────────────────┘  │
│  ...                                             │
└──────────────────────────────────────────────────┘
```

### Detail-näkymä (klikkauksen jälkeen)

```
┌──────────────────────────────────────────────────┐
│  ← Back to list                                  │
│                                                  │
│  Re: Q1 Budget Review                            │
│  ────────────────────────────────────────────    │
│  From:    Maria Chen <maria@example.com>         │
│  To:      team@example.com                       │
│  Date:    2026-04-06 10:32                       │
│                                                  │
│  Thanks for the updated numbers. I've reviewed   │
│  the projections and have a few comments:        │
│                                                  │
│  1. The Q2 forecast looks conservative...        │
│  2. Marketing spend should include...            │
│                                                  │
└──────────────────────────────────────────────────┘
```

## List + suodatukset

List-section reagoi suodatuksiin kuten muutkin:
- Chart suodattaa `from`-kentällä → lista näyttää vain sen lähettäjän viestit
- Suodatettu count näkyy otsikossa: "Inbox (12 / 142 messages)"

List voi itsessään olla suodatin:
```yaml
  - title: "Inbox"
    type: list
    source: emails
    interactive: true
    filter_field: from           # listan kohteen klikkaus suodattaa
```

## Muut layout-vaihtoehdot

### `rows` — tiiviimpi, taulukkomainen

```
│ Re: Q1 Budget Review        Maria Chen     2h ago  │
│ Deploy notification v2.4.1  deploy-bot     4h ago  │
│ Weekly standup notes         Jari M.       1d ago  │
```

### `compact` — pelkkä otsikko

```
│ • Re: Q1 Budget Review (Maria Chen, 2h ago)       │
│ • Deploy notification v2.4.1 (deploy-bot, 4h ago) │
```

## Client-side arkkitehtuuri

### List-renderöinti

```javascript
function renderList(section, data) {
  // Renderöi lista-kortteja
  // Jokainen kortti on klikattava
  // Klikkaus → detail-näkymä (korvaa listan tässä section-cardissa)
}

function renderDetail(section, item) {
  // Renderöi yksittäisen kohteen
  // "← Back" -linkki palaa listaan
  // body_field renderöidään HTML:nä
}
```

### Navigointimalli

Detail-näkymä EI avaa uutta sivua — se korvaa section-cardin sisällön.
Tämä pitää dashboardin muut osat näkyvissä ja suodatukset aktiivisina.

Vaihtoehto: `detail_mode: overlay` avaa modal-dialogin section-cardin päälle.

```yaml
    list:
      detail_mode: replace          # replace (oletus) | overlay | fullscreen
```

## Vaikutus arkkitehtuuriin

**YAML-spec**: uusi `list`-section-tyyppi, `list`-konfiguraatio-objekti.

**Renderer**: uusi `render_list_section()` joka generoi JS-koodin
listan renderöintiin, klikkauskäsittelyyn ja detail-navigaatioon.

**Data**: list käyttää samaa Dataset-mekanismia kuin chart ja table.
Datan pitää sisältää kaikki kentät joita list ja detail tarvitsevat.
