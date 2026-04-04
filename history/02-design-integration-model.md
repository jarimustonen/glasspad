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

```
POST /api/pads
{
  "type": "html" | "chart" | "markdown" | "table" | "dashboard",
  "content": "..." | { spec },
  "title": "optional title",
  "ttl": 3600
}
→ { "id": "abc123", "url": "http://localhost:3000/abc123" }
```

```
PUT /api/pads/:id
{
  "content": "updated content"
}
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

## Sisältötyypit

| Tyyppi | Agentti lähettää | Glasspad renderöi |
|--------|-----------------|-------------------|
| `html` | Raw HTML (+ CSS/JS) | Sandboxed iframe |
| `chart` | Vega-Lite JSON spec | Vega-Lite → SVG/Canvas |
| `markdown` | Markdown-teksti | Rendered HTML |
| `table` | JSON array / CSV | Interaktiivinen taulukko |
| `dashboard` | Array of pads | Grid-layout, useita näkymiä |

## Turvallisuusmalli

- **Localhost-only** oletuksena — ei tarvetta raskaalle suojaukselle
- HTML renderöidään suoraan (ei sandboxia) — sisältö tulee käyttäjän omalta agentilta
- Pad-ID:t ovat UUID:eja — arvaamattomat
- TTL-vanheneminen oletuksena (esim. 24h)
- Ei autentikaatiota — paikallinen työkalu
- Sandbox optiona myöhemmin jos tuetaan julkista jakelua
