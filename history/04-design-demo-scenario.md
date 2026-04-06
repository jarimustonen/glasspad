# Design: Demo-skenaario

## Demo: Git-projektin tilannekuva

Agentti analysoi käyttäjän git-repoa ja luo dashboardin jossa näkyy:
- Commitit per päivä (bar chart)
- Tiedostotyyppien jakauma (pie chart)
- Koodirivit moduuleittain (horizontal bar)
- Testikattavuuden trendi (line chart)
- Viimeisimmät commitit (taulukko)
- Yhteenveto (KPI-kortit)

---

## 1. Agentti tuottaa YAML-specin

Agentti ei generoi HTML:ää — se tuottaa rakenteisen YAML-kuvauksen siitä mitä
haluaa näyttää. Glasspad renderöi YAMLista dashboardin.

Katso esimerkki: `history/examples/git-dashboard.yaml`

### YAML-specin rakenne

```yaml
title: "Dashboard title"
description: "Optional description"
layout: grid-2col                    # asettelu

sections:
  - title: "Section title"
    type: chart                      # chart | table | stats
    chart:
      mark: bar | arc | line         # kaaviotyyppi
      data: [...]                    # data inline
      encoding:                      # Vega-Lite -henkinen
        x: { field: ..., type: ... }
        y: { field: ..., type: ... }

  - title: "Table section"
    type: table
    columns: [...]
    data: [...]

  - title: "KPI cards"
    type: stats
    data:
      - { label: "Metric", value: 42 }
```

### Miksi YAML eikä HTML

- Agentti tuottaa YAMLia luontevasti, pienempi token-kulutus
- Glasspad hallitsee ulkoasun — yhtenäinen tyyli kaikissa padeissa
- Helppo validoida ja debugata
- Ihmisen luettavissa sellaisenaan
- Raw HTML edelleen tuettuna erikoistapauksiin

---

## 2. API-kutsut

### Luonti YAML-specistä

```bash
curl -X POST http://localhost:3000/api/pads \
  -H "Content-Type: application/x-yaml" \
  --data-binary @dashboard.yaml
```

Vastaus:
```json
{
  "id": "a1b2c3d4",
  "url": "http://localhost:3000/a1b2c3d4",
  "title": "weatherapi — Git Dashboard",
  "created_at": "2026-04-04T12:00:00Z",
  "expires_at": "2026-04-05T12:00:00Z"
}
```

### Luonti JSON-bodystä (vaihtoehto)

```bash
curl -X POST http://localhost:3000/api/pads \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Quick note",
    "type": "html",
    "content": "<h1>Hello</h1>"
  }'
```

### Päivitys

```bash
curl -X PUT http://localhost:3000/api/pads/a1b2c3d4 \
  -H "Content-Type: application/x-yaml" \
  --data-binary @updated.yaml
```

### Listaus

```bash
curl http://localhost:3000/api/pads
```

```json
[
  {
    "id": "a1b2c3d4",
    "title": "weatherapi — Git Dashboard",
    "type": "dashboard",
    "created_at": "2026-04-04T12:00:00Z"
  }
]
```

---

## 3. CLI-työkalu

```bash
# Luo pad YAML-tiedostosta
glasspad create --file dashboard.yaml
# → Created pad a1b2c3d4
# → http://localhost:3000/a1b2c3d4

# Luo pad stdinistä (agentin luonnollinen tapa)
cat dashboard.yaml | glasspad create
# → Created pad a1b2c3d4
# → http://localhost:3000/a1b2c3d4

# Raw HTML erikoistapauksiin
echo '<h1>Hello</h1>' | glasspad create --type html --title "Quick"

# Päivitä
glasspad update a1b2c3d4 --file updated.yaml

# Listaa
glasspad list
# ID        TITLE                        TYPE       CREATED
# a1b2c3d4  weatherapi — Git Dashboard   dashboard  2 min ago

# Avaa selaimessa
glasspad open a1b2c3d4

# Poista
glasspad delete a1b2c3d4
```

CLI tunnistaa formaatin automaattisesti:
- `.yaml` / `.yml` → glasspad dashboard spec
- `.html` → raw HTML
- `.md` → markdown
- `.json` → Vega-Lite spec tai data
- `.csv` → taulukkodata

---

## 4. Claude Code Skill

### `.claude/skills/glasspad.md`

```markdown
# Glasspad — Visual Output

Show visual content (charts, tables, dashboards) to the user via glasspad.

## When to use

- User asks to visualize, plot, chart, or dashboard something
- You want to show complex results visually
- Data has multiple dimensions that benefit from visual presentation

## How to use

1. Collect and analyze the data
2. Write a YAML spec describing what to show
3. Pipe it to glasspad:

\`\`\`bash
cat <<'YAML' | glasspad create
title: "Dashboard title"
layout: grid-2col

sections:
  - title: "Chart title"
    type: chart
    chart:
      mark: bar
      data:
        - { x: "A", y: 10 }
        - { x: "B", y: 20 }
      encoding:
        x: { field: x, type: nominal }
        y: { field: y, type: quantitative }

  - title: "Data table"
    type: table
    columns:
      - { field: name, title: "Name" }
      - { field: value, title: "Value" }
    data:
      - { name: "Alpha", value: 42 }

  - title: "Key metrics"
    type: stats
    data:
      - { label: "Total", value: 1234 }
      - { label: "Average", value: "56.7%" }
YAML
\`\`\`

4. Tell the user the URL so they can open it in their browser

## Section types

- **chart** — bar, line, arc (pie), with mark + data + encoding
- **table** — columns + data rows
- **stats** — label/value pairs shown as KPI cards

## Layouts

- `grid-2col` — two columns
- `grid-3col` — three columns
- `stack` — single column, stacked vertically

## Tips

- Always include a descriptive title
- Keep data inline in the YAML — no external file references
- For updates: `glasspad update <id> --file updated.yaml`
- Use stats sections for summary numbers
- Use raw HTML only when YAML sections can't express what you need:
  `echo '<html>...</html>' | glasspad create --type html`
```

---

## 5. Demo-flow kokonaisuutena

```
Käyttäjä: "Näytä tämän projektin git-statistiikka dashboardina"

Agentti:
  1. git log, find, wc -l → kerää data
  2. Koostaa YAML-specin (sections: chart, chart, table, stats)
  3. cat <<'YAML' | glasspad create
     ...yaml spec...
     YAML
  4. "Dashboard luotu: http://localhost:3000/a1b2c3d4"

Käyttäjä avaa URL:n → näkee renderöidyn dashboardin selaimessa
```

---

## 6. Mitä tämä demo testaa

- [x] YAML-spec → HTML-renderöinti pipeline
- [x] Sisältötyypit: chart (bar, arc, line), table, stats
- [x] CLI stdin-putkitus
- [x] Automaattinen formaattitunnistus
- [x] Grid-layout
- [x] Skill-pohjainen agenttiohjaus
- [ ] Päivitys ja live-refresh (SSE) — demovaihe 2
- [ ] Kaksisuuntaisuus (callback) — demovaihe 3
