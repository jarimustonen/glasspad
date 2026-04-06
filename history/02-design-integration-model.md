# Design: Integraatiomalli tekoälyjen kanssa

## Konseptitason arkkitehtuuri

```
┌─────────────┐     POST /api/pads      ┌──────────────┐     GET /:id      ┌──────────┐
│  AI Agent   │ ──────────────────────→  │   Glasspad   │ ←───────────────  │  Selain  │
│ (Claude,    │ ←──────────────────────  │   Server     │ ──────────────→  │  (User)  │
│  OpenClaw,  │   { pad_id, url }        │              │   rendered HTML   │          │
│  GPT, ...)  │                          │              │                   │          │
└─────────────┘                          └──────────────┘                   └──────────┘
      ▲                                        │                                 │
      │              callback / event          │         user interaction         │
      └────────────────────────────────────────┴─────────────────────────────────┘
```

## Kommunikaatiovirrat

### 1. Agent → Glasspad (sisällön luonti ja päivitys)

**Ensisijainen: YAML-spec** (agentti kuvaa mitä näyttää, glasspad renderöi)

```
POST /api/pads
Content-Type: application/x-yaml

title: "Dashboard title"
layout: grid-2col
sections:
  - title: "Chart"
    type: chart
    chart: { mark: bar, data: [...], encoding: {...} }
  - title: "Table"
    type: table
    columns: [...]
    data: [...]

→ { "id": "abc123", "url": "http://localhost:3000/abc123" }
```

**Vaihtoehto: raw HTML** (erikoistapaukset)

```
POST /api/pads
Content-Type: application/json

{ "type": "html", "title": "...", "content": "<html>...</html>" }
→ { "id": "abc123", "url": "http://localhost:3000/abc123" }
```

```
PUT /api/pads/:id
Content-Type: application/x-yaml

(päivitetty YAML-spec)
```

### 2. Glasspad → Selain (renderöinti ja live-päivitykset)

- Ensimmäinen lataus: palvelin renderöi HTML-sivun
- Live-päivitykset: **SSE** (Server-Sent Events) yhteys
  - Selain kuuntelee: `GET /api/pads/:id/events`
  - Kun agentti tekee PUT, selain saa päivityksen automaattisesti

### 3. Selain → Agent (callback — kaksisuuntaisuus)

Tämä on **glasspadin erottava ominaisuus**: pad voi kommunikoida takaisin agentille.

**Mekanismi: Event queue**

```
Selain:  POST /api/pads/:id/events  { "type": "click", "data": { "selected": "row-5" } }
Agent:   GET  /api/pads/:id/events?since=<cursor>  → [{ events }]
         tai
         GET  /api/pads/:id/events (SSE stream)
```

**Käyttötapaukset:**
- Käyttäjä valitsee datapisteitä chartista → agentti analysoi tarkemmin
- Käyttäjä täyttää lomakkeen padissa → agentti käsittelee
- Käyttäjä klikkaa "hyväksy/hylkää" → agentti jatkaa työnkulkua
- Käyttäjä muokkaa parametreja (slider) → agentti laskee uudelleen

**Arkkitehtuurivalinta:** Polling + SSE, ei WebSocketeja
- Yksinkertaisempi toteuttaa ja debugata
- AI-agentit osaavat HTTP:n luontevasti
- SSE riittää reaaliaikaiseen päivitykseen

### 4. MCP-integraatio (Claude Code / OpenClaw)

MCP-serveri käärii REST APIn tool-kutsuiksi:

```
Tools:
  - create_pad(type, content, title?) → { id, url }
  - update_pad(id, content) → ok
  - get_pad_events(id, since?) → [events]
  - list_pads() → [{ id, title, type, created_at }]
  - delete_pad(id) → ok
```

Agentti voi siis:
1. Luoda padin → käyttäjä saa URL:n terminaaliin
2. Kuunnella käyttäjän interaktioita → reagoida niihin
3. Päivittää padia iteratiivisesti

### 5. CLI-integraatio

```bash
# Erillisenä komentona
glasspad create --type chart --file spec.json
glasspad update abc123 --file updated.html
glasspad list
glasspad events abc123 --follow

# Tai Claude Coden sisältä MCP:n kautta
```

## Sisältöformaatit

**Ensisijainen: YAML-spec** — agentti kuvaa rakenteen, glasspad renderöi

| Section type | Agentti kuvaa YAMLissa | Glasspad renderöi |
|-------------|----------------------|-------------------|
| `chart` | mark + data + encoding | Vega-Lite → SVG/Canvas |
| `table` | columns + data | Interaktiivinen taulukko |
| `stats` | label/value -parit | KPI-kortit |

**Vaihtoehto: raw-formaatit** — erikoistapauksiin

| Formaatti | Käyttö |
|-----------|--------|
| `html` | Täysi vapaus, agentti tuottaa valmiin HTML:n |
| `markdown` | Tekstipainotteinen sisältö |
| `json` | Suora Vega-Lite spec |
| `csv` | Taulukkodata |

## Turvallisuusmalli

- **Localhost-only** oletuksena — ei tarvetta raskaalle suojaukselle
- HTML renderöidään suoraan (ei sandboxia) — sisältö tulee käyttäjän omalta agentilta
- Pad-ID:t ovat UUID:eja — arvaamattomat
- TTL-vanheneminen oletuksena (esim. 24h)
- Ei autentikaatiota — paikallinen työkalu
- Sandbox optiona myöhemmin jos tuetaan julkista jakelua
