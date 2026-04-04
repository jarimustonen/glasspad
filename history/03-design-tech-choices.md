# Design: Teknologiavalinnat

## Kieli: Rust

**Perustelu:** Glasspad on dev-työkalu jonka pitää "vain toimia". Single binary,
ei runtime-riippuvuuksia, nopea käynnistys. Serveri on yksinkertainen (tallenna,
tarjoile, streamaa) — ei tarvitse JS-ekosysteemiä. Renderöinti tapahtuu selaimessa.

## Stack

| Komponentti | Valinta | Perustelu |
|-------------|---------|-----------|
| Web framework | **Axum** | Tokio-pohjainen, modulaarinen, hyvin tuettu |
| CLI | **Clap** | De facto standardi Rust CLI:lle |
| Tietokanta | **SQLite** (rusqlite) | Yksinkertainen, ei erillistä prosessia, riittävä |
| Serialisointi | **serde + serde_json** | Standardi |
| SSE | Axum:n sisäänrakennettu SSE-tuki | |
| UUID | **uuid** crate | Pad-ID:t |
| Templating | **askama** tai inline HTML | Kevyt, compile-time |

## Jakelu

- `cargo install glasspad`
- Yksi binääri sisältää: serveri + CLI + MCP-serveri
- Ei Node.js-, Python- tai muita runtime-riippuvuuksia

## Selainpuoli (staattinen, upotettu binääriin)

- Vega-Lite (chart-renderöinti)
- Markdown-renderöinti (marked.js tai vastaava)
- Minimaalinen CSS (ei frameworkia)
- Ladataan CDN:stä tai upotetaan `include_bytes!`

## MCP-serveri

- Samassa binäärissä: `glasspad serve --mcp` tai `glasspad mcp`
- Rust MCP -kirjasto (rmcp tai oma stdio-toteutus)
- Protokolla on yksinkertainen JSON-RPC stdio:n yli
