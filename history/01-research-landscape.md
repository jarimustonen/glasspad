# Tutkimus: Markkina- ja teknologiakatsaus

## A) Dashboard- ja visualisointikirjastot (open source)

### Täydet dashboard-alustat
| Työkalu | Fokus | Huomiot |
|---------|-------|---------|
| **Grafana** | Monitorointi, aikasarjat | Plugin-ekosysteemi, kymmeniä datalähteitä |
| **Apache Superset** | Data exploration | 40+ visualisointityyppiä, plugin-arkkitehtuuri |
| **Metabase** | Business intelligence | Natural language -kyselyt, AI-tuettu |
| **Kibana** | Lokit ja tapahtumat | Elastic Stack -integraatio |

### JavaScript-visualisointikirjastot
| Kirjasto | Taso | Vahvuus |
|----------|------|---------|
| **D3.js** | Matala | Täysi kontrolli, bespoke-visualisoinnit |
| **Observable Plot** | Korkea | D3-tiimin tekemä, tiivis API, nopea prototypointi |
| **Chart.js** | Korkea | 2.5M+ npm latauksia/vko, canvas-pohjainen, helppo |
| **Vega-Lite** | Korkea | Deklaratiivinen grammar, JSON-spec → kuva |
| **Plotly.js** | Korkea | Interaktiiviset 3D/tilastograafit |
| **ECharts** (Apache) | Korkea | Laaja valikoima, suorituskykyinen |

**Glasspad-relevanssi:** Emme rakenna dashboardia vaan *renderöintialustaa*. Kirjastoista
kiinnostavimmat ovat ne, jotka toimivat deklaratiivisesti (JSON → kuva):
- **Vega-Lite** — AI tuottaa JSON-specin, glasspad renderöi
- **Chart.js** — yksinkertainen, laaja tuki
- **Observable Plot** — moderni, tiivis

## B) Olemassa olevat AI-visuaaliset tuotokset

### Claude Artifacts
- AI generoi HTML/JS-koodia, renderöidään live side-panelissa
- "Generative UI": interaktiiviset komponentit (lomakkeet, dashboardit, laskelmat)
- Rajoitus: toimii vain claude.ai-webissä, ei CLI:ssä (Claude Code)

### OpenAI Canvas
- Kollaboratiivinen editori dokumenteille ja koodille
- EI live-renderöintiympäristö — keskittyy tekstin/koodin muokkaukseen

### Open WebUI
- Self-hosted, tukee Ollama + OpenAI-yhteensopivia API:eja
- **Artifacts**: persistent key-value storage, live HTML/CSS/JS preview
- Python Code Interpreter sisäänrakennettuna

### LibreChat
- Käyttää CodeSandbox Sandpackia HTML/JS:n renderöintiin
- Chart-renderöinti reaaliajassa erillisessä ikkunassa

### Google A2UI (Agent-to-UI)
- **Deklaratiivinen UI-protokolla**: agentti lähettää komponenttikuvauksia (JSON)
- Ei suoritettavaa koodia — UI datana, ei koodina → turvallisuus
- Cross-platform: sama JSON renderöityy webissä, mobiilissa, desktopissa
- v0.8 Public Preview, Apache 2 -lisenssi
- **Erittäin relevantti glasspadin kannalta** — samansuuntainen filosofia

### MCP Apps (Claude Code)
- Sandboxed iframe -widgetit suoraan chatissa
- Lomakkeet, valitsimet, vahvistus-dialogit, kaaviot, live-statuspäivitykset

### n8n + QuickChart
- Agentti tuottaa strukturoitua dataa → QuickChart renderöi kuvan
- Esimerkki: OpenAI Structured Output → JSON → chart-URL

## C) Ominaisuudet joita glasspad voisi tarjota

### Perusominaisuudet (MVP)
1. **HTML/JS pad** — agentti POST:aa HTML:ää, käyttäjä avaa URL:n
2. **Chart pad** — agentti POST:aa Vega-Lite/Chart.js JSON-specin
3. **Markdown pad** — rich text -renderöinti
4. **Data table** — taulukkomuotoinen data, lajittelu, suodatus
5. **Pad-URL jakaminen** — yksinkertainen linkki, ei kirjautumista

### Kehittyneet ominaisuudet
6. **Live update** — agentti päivittää padia (SSE/WebSocket), näkyy reaaliajassa
7. **Pad → Agent callback** — käyttäjä klikkaa/valitsee padissa, tieto palaa agentille
8. **Multi-view dashboard** — useita padeja yhdessä näkymässä
9. **Deklaratiivinen komponenttikirjasto** — A2UI-henkinen: agentti kuvaa UI:n JSONina
10. **Interaktiiviset kontrollit** — lomakkeet, sliderit, valinnat → data takaisin agentille

### Integraatio-ominaisuudet
11. **MCP-serveri** — Claude Code / OpenClaw käyttää natiivisti
12. **CLI-työkalu** — `glasspad create`, `glasspad update`, `glasspad list`
13. **REST API** — universaali, mikä tahansa agentti voi käyttää
14. **Upotettavat widgetit** — iframe-embed muihin sovelluksiin

### Turvallisuus
15. **Sandboxed rendering** — iframe sandbox / CSP, ei pääsyä hostiin
16. **Padien vanheneminen** — automaattinen TTL
17. **Sisältövalidointi** — estä XSS, script injection

## Lähteet

- [MetricFire: Open Source Dashboards 2026](https://www.metricfire.com/blog/top-8-open-source-dashboards/)
- [FusionCharts: 20 Best JS Visualization Libraries](https://www.fusioncharts.com/blog/best-javascript-data-visualization-libraries-2/)
- [D3.js](https://d3js.org/)
- [Observable Plot](https://observablehq.com/plot/)
- [LibreChat Artifacts](https://www.librechat.ai/docs/features/artifacts)
- [Claude Artifacts](https://support.claude.com/en/articles/11649427-use-artifacts-to-visualize-and-create-ai-apps-without-ever-writing-a-line-of-code)
- [MindStudio: Claude Generative UI vs Canvas](https://www.mindstudio.ai/blog/what-is-claude-generative-ui-vs-canvas-artifacts)
- [Google A2UI](https://a2ui.org/)
- [A2UI Protocol Guide](https://dev.to/czmilo/the-a2ui-protocol-a-2026-complete-guide-to-agent-driven-interfaces-2l3c)
- [Open WebUI](https://docs.openwebui.com/features/)
- [n8n AI Agent Charts](https://n8n.io/workflows/2400-ai-agent-with-charts-capabilities-using-openai-structured-output-and-quickchart/)
