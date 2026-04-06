# Review: Integration commit (ccfa853)

**Reviewed:** src/models.rs, store.rs, routes/api.rs, routes/render.rs, renderer.rs, cli.rs, main.rs, csp.rs
**Reviewers:** Gemini, Codex
**Rounds:** 2

---

## Critical Issues (Consensus)

### 1. XSS: Vega-Lite spec JSON injected into inline `<script>` without escaping

- **What:** `spec_json` sisältää käyttäjädataa ja injektoidaan suoraan `<script>`-tägiin. Jos data sisältää `</script>`, selain sulkee script-blokin ja suorittaa hyökkääjän koodin. CSP `unsafe-inline` mahdollistaa tämän.
- **Where:** `src/renderer.rs:196-226`
- **Fix:** Käytä samaa `safe_json_script_tag`-tekniikkaa Vega-specille: erillinen `<script type="application/json">` per chart, JS parsii sen.

### 2. API ei käytä top-level datasettejä — `collect_inline_datasets` hakee vain section.inline_data:sta

- **What:** Kanoninen spec (`datasets: { events: {} }` + `source: events`) ei toimi serverillä. `collect_inline_datasets` ignoroi spec.datasets kokonaan ja yrittää rekonstruoida datasetit section.inline_data:sta.
- **Where:** `src/routes/api.rs:21-34`
- **Fix:** Serverin pitää tukea top-level datasettejä suoraan. CLI:n ei pitäisi rewrite sourceja inline_dataksi.

### 3. CLI --data monistaa datasetin jokaiseen section:iin

- **What:** Jos 5 sectionia viittaa samaan `source: events`, CLI injektoi 5 kopiota datasta inline_data:ksi. 10MB CSV → 50MB payload.
- **Where:** `src/cli.rs:155-184`
- **Fix:** CLI:n pitäisi injektoida data top-level `datasets`-mappiin, ei per-section inline_dataan.

### 4. CLI ei tallenna/näytä tokenia — update/delete mahdotonta

- **What:** `PadCreated.token` palautetaan serveriltä mutta CLI:n `handle_create_response` hylkää sen. Pad-tokenin menettäminen tekee PUT/DELETE-operaatioista mahdottomia.
- **Where:** `src/cli.rs:143`
- **Fix:** Tulosta token stderriin: `eprintln!("Token: {}", created.token);`

### 5. PadStore::get() kloonaa koko Padin datasetteineen

- **What:** Jokainen HTTP GET kloonaa koko dataset-datan. 50MB CSV = 50MB allokointia per pyyntö.
- **Where:** `src/store.rs:24-27`
- **Fix:** `HashMap<String, Arc<Pad>>`, palauta `Arc<Pad>`.

---

## Important Issues

### 6. Duplikaattidataset-avaimet ylikirjoittuvat hiljaisesti

- **Where:** `routes/api.rs:28-33`
- **Fix:** Havaitse duplikaatit ja palauta virhe.

### 7. ensure_server hyväksyy minkä tahansa HTTP-vastauksen

- **Where:** `cli.rs:12`
- **Fix:** Tarkista `resp.status().is_success()`.

### 8. Content sniffing on hauras

- **Where:** `routes/api.rs:56-62`
- **Fix:** Luota Content-Type-headeriin ja parsintaan, älä tarkista ensimmäistä avainta.

### 9. Stats: sum palauttaa 0 tyhjällä (pitäisi "—"), float epsilon liian tiukka, min/max jättää ei-numerot huomiotta

- **Where:** `renderer.rs:341-380`
- **Fix:** sum tyhjällä → "—", epsilon → `==` tai suhteellinen toleranssi.

### 10. Datasets upotetaan sekä `glasspad-data`-tägiin ETTÄ Vega-speceihin — kaksinkertainen data

- **Where:** `renderer.rs:49` + `renderer.rs:196`
- **Fix:** Poista `glasspad-data`-upotus kunnes client-side tarvitsee sitä, tai käytä vain yhtä tapaa.

### 11. unwrap()-kutsuja käyttäjäpoluilla

- **Where:** `cli.rs` (useita kohtia), `renderer.rs`
- **Fix:** Käsittele virheet.

---

## Minor Issues

- Tiedostopäätetunnistus case-sensitive (`"csv"` vs `"CSV"`)
- Duplikaatti `--data` -argumentit hyväksytään
- `dataset_meta` tallennetaan mutta ei käytetä
- `CellValue::to_string()` saattaa tuottaa yllättäviä tuloksia taulukoissa — käytä dedikoitua format_cell-funktiota

---

## Moderaattorin yhteenveto

Molemmat arvioijat löysivät samat ydinongelman eri kulmista:

**Perusongelma on yksinkertainen:** CLI muokkaa spec:iä väärin (per-section inline_data), ja serveri yrittää rekonstruoida datasettejä tästä väärästä esityksestä. Oikea ratkaisu:

1. **Serveri**: tue top-level `datasets` suoraan `collect_datasets`:ssa
2. **CLI**: injektoi data `datasets`-mappiin, jätä `source` koskemattomaksi
3. **Renderer**: ratkaise data `source`→`pad.datasets` -viittauksella

XSS on toinen kriittinen löydös — Vega-specit pitää upottaa samalla `safe_json_script_tag`-tekniikalla kuin datasets.

Token-ongelma on helppo korjata mutta estää koko auth-mallin toiminnan.
