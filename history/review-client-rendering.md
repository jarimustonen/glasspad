# Review: Client-side rendering (commit 60ea300)

**Reviewed:** src/renderer.rs (client-side JS rewrite)
**Reviewers:** Gemini, Codex
**Rounds:** 2

---

## Critical Issues (Consensus)

### 1. Schema field name mismatch todennäköisesti rikkoo kaiken

- **What:** JS käyttää `section.type` ja `item.where`, mutta Rust saattaa serialisoida `section_type` ja `where_clause`. Jos `#[serde(rename)]` ei ole asetettu, kaikki sectionit renderöidään "Unknown section type: undefined".
- **Where:** CLIENT_JS, `switch (section.type)` ja `computeAggregate()`
- **Fix:** Tarkista Rust-schema: `#[serde(rename = "type")]` on jo `Section.section_type`:lla. `StatsItem.where_clause` on `#[serde(rename = "where")]`. Jos molemmat ovat kunnossa → ei bugia. Jos ei → heti rikki.

### 2. Vega-view-instanssit hylätään — estää tehokkaan suodatuksen

- **What:** `vegaEmbed()` palauttaa View-instanssin mutta se hylätään. Suodatus vaatii view:n päivittämistä datalla. Ilman sitä jokainen suodatusmuutos vaatii koko chartin uudelleenluonnin.
- **Where:** CLIENT_JS, `renderChart()`
- **Fix:** Tallenna view-instanssit Map:iin: `chartViews.set(sectionKey, result.view)`

### 3. `Math.min.apply` / `Math.max.apply` kaatuu isoilla dataseteillä

- **What:** JS:n argumenttimäärä rajoitettu (~65k). 50k rivin dataset → RangeError.
- **Where:** CLIENT_JS, `computeAggregate()` min/max
- **Fix:** Käytä `reduce()` tai looppia.

### 4. `innerHTML +=` on O(N²) ja tuhoaa DOM-kontekstin

- **What:** Toistuva `innerHTML +=` parsii koko alipuun uudelleen joka iteraatiolla. Tuhoaa event listenerit ja vaikeutttaa suodatuksen lisäämistä.
- **Where:** CLIENT_JS, `renderInlineStats`, `renderAggregateStats`, virheviestit
- **Fix:** Käytä DOM API:a (`createElement`/`textContent`) tai kokoa HTML kerran ja aseta `innerHTML` yhdellä kertaa.

### 5. JS Rust-stringissä — ei lintausta, ei testejä, ei syntaksiväritystä

- **What:** ~200 riviä JS:ää `const &str`:ssä. Kasvaa suodatuksella. Mahdoton ylläpitää.
- **Where:** `src/renderer.rs`, `const CLIENT_JS`
- **Fix:** `include_str!("../static/dashboard.js")` + erillinen tiedosto.

---

## Important Issues

### 6. Ei bootstrap-virheenkäsittelyä
JSON.parse tai puuttuva elementti kaataa koko sivun hiljaisesti. → Wrap `try/catch` + näytä virhe.

### 7. Aggregaattiformatointi muuttui
`Math.round(sum)` pyöristää desimaalit pois. Vanha käytös: kokonaisluvut ilman desimaaleja, muut yhdellä desimaalilla. → Port `formatDecimal` tarkasti.

### 8. `to_value(spec).unwrap_or(Null)` piilottaa virheet
Serialisointivirhe → JS saa `null` → kaatuu. → Epäonnistu äänekkäästi.

### 9. `esc()` allokoi DOM-elementin per kutsu
10 000 solua = 10 000 `createElement` kutsua. → String-pohjainen escape.

### 10. cell_to_json ei käsittele NaN/Infinity
`serde_json::json!(NaN)` → paniikki tai null. → Eksplisiittinen tarkistus.

---

## Minor Issues
- `distinct` deduplikointisemantikka eroaa hieman (String(v) vs formatCell(v))
- null vs undefined vertailusemantikka where-lauseissa
- Taulukoissa ei rivirajaa (isoilla dataseteillä hidas)
- Tyhjä data vs puuttuva dataset ei erotu (molemmat → [])

---

## Moderaattorin yhteenveto

**Codex** löysi kriittisimmän ongelman (schema field name mismatch) jonka Gemini ohitti. **Gemini** löysi Math.min.apply bugin ja Vega-view-lifecycle ongelman ensimmäisenä.

**Tarkistetaan heti:** onko schema rename kunnossa. Jos `section_type` serialisoituu `"type"`:ksi ja `where_clause` serialisoituu `"where"`:ksi, suurin kriittinen ongelma on jo ratkaistu.

**Tärkein korjaus suodatusta varten:** Vega-view-instanssien tallennus + `include_str!` JS:lle.
