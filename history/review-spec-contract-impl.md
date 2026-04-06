# Review: Spec Contract Implementation (commit 4c6b0ac)

**Reviewed:** src/data/, src/spec/, src/security/, tests/
**Reviewers:** Gemini (gemini-3.1-pro-preview), Codex (GPT-5.4)
**Rounds:** 2

---

## Critical Issues (Consensus)

### 1. XSS via json_embed.rs — case-insensitive script tag bypass

- **What:** Escaping handles only `</script>` and `</SCRIPT>`, mutta HTML-parserit ovat case-insensitive. `</sCrIpT>` ohittaa suojauksen.
- **Where:** `src/security/json_embed.rs:19-22`
- **Why it matters:** Datassa oleva hyökkäyskoodi suoritetaan selaimessa.
- **Fix:** Korvaa kaikki `<` merkit: `json.replace('<', "\\u003c")`

### 2. JSON-parser korruptoi dataa kahdella tavalla

- **What:**
  a) `unwrap_or(0.0)` muuttaa out-of-range numerot hiljaisesti nollaksi
  b) JSON-merkkijonot ajetaan `infer_cell_value`:n läpi: `"42"` → `Number(42.0)`, `"true"` → `Bool(true)`
- **Where:** `src/data/json.rs:33-34` ja `src/data/json.rs:38`
- **Why it matters:** Data menettää semanttisen merkityksensä — JSON:n eksplisiitit tyypit ylikirjoitetaan.
- **Fix:** a) Palauta virhe `unwrap_or(0.0)` sijaan. b) JSON-merkkijonot säilytetään sellaisinaan: `Ok(CellValue::String(s.clone()))`

### 3. Spec-validaattori liian heikko ja sisältää kuollutta koodia

- **What:**
  a) Dead code: `provided_datasets` -haara on mahdoton (ehto aina false)
  b) `chart.encoding` ei-Object (esim. `123` tai `[]`) ohittaa validoinnin hiljaisesti
  c) Non-chart `interactive_filter` hyväksytään ilman virhettä/varoitusta
  d) Puuttuu: section ID -uniikkius, tyhjät stats.items, väärä config-tyyppi sectionille
- **Where:** `src/spec/validate.rs:68-72`, `validate_chart()`, passim
- **Fix:** Poista dead code. Lisää virhe jos encoding ei ole Object. Vahvista cross-field validointi.

### 4. CSV-koon rajoitusta ei oikeasti valvota

- **What:** `parse_csv<R: Read>` hyväksyy minkä kokoisen syötteen tahansa. `MAX_CSV_BYTES` on vain vakio jota kukaan ei tarkista.
- **Where:** `src/data/csv.rs:29`
- **Why it matters:** DoS: yksi iso tiedosto kaataa serverin muistinpuutteeseen.
- **Fix:** Wrap reader: `reader.take(MAX_CSV_BYTES as u64)`, tarkista luettu koko.

### 5. deny_unknown_fields puuttuu spec-tyypeistä

- **What:** YAML-specin kirjoitusvirheet (esim. `enccoding:`, `sectons:`) ohitetaan hiljaisesti.
- **Where:** `src/spec/schema.rs` — kaikki structit
- **Why it matters:** AI-agentit tuottavat specejä — kirjoitusvirheiden hiljaisesti katoaminen tuottaa mystisiä bugeja.
- **Fix:** Lisää `#[serde(deny_unknown_fields)]` päästructeihin.

---

## Disputed Issues

### 6. Muistin käyttö: Vec<BTreeMap> vs. columnar Dataset

- **Gemini:** Jokainen solu allokoi String-avaimen ja tree-noden. 50MB CSV → >500MB RAM. Siirry columnar-malliin.
- **Codex:** Columnar on parempi pitkällä aikavälillä, mutta MVP:lle `Vec<Row>` riittää kun on kokorajoitukset.
- **Moderaattorin arvio:** Molemmat ovat oikeassa eri konteksteissa. MVP:ssä `Vec<Row>` riittää dokumentoiduilla rajoituksilla (≤10k riviä). Mutta `Dataset`-tyyppi kannattaa kääriä structiin (jossa `headers: Vec<String>`) jo nyt, jotta columnar-migraatio on myöhemmin helppo.

### 7. CSV:n puuttuvat sarakkeet: Null vs. puuttuva avain

- **Codex:** Puuttuvat trailing-sarakkeet pitäisi täyttää Nullilla.
- **Gemini:** BTreeMapissa puuttuva avain = None lookupissa = sama kuin Null.
- **Moderaattorin arvio:** Codex on oikeassa. Kanoninen taulukkodatamalli: jokainen rivi sisältää kaikki sarakkeet. Muuten metadata-inferenssi ja suodatus ovat epäluotettavia.

### 8. Constant-time token vertailu

- **Gemini:** Käytä `std::hint::black_box`. **Codex:** `black_box` ei ole kryptografinen primitiivi, käytä `subtle`-kirjastoa.
- **Moderaattorin arvio:** Codex on oikeassa. `subtle::ConstantTimeEq` on oikea ratkaisu. `black_box` on benchmarking-työkalu.

---

## Minor Findings

- `infer_cell_value` trimmaa whitespace-arvoja (`" Alice "` → `"Alice"`) — problemaattinen CSV-datalle
- `is_temporal` hyväksyy `"2026-99-99"` ja `"2026-04-06Tgarbage"` — riittänee MVP:lle mutta tiukentaminen suositeltavaa
- `Display for CellValue::Number`: `*n as i64` -cast on vaarallinen isoilla luvuilla
- Error-tyypit eivät implementoi `std::error::Error` — hankaloittaa Axum-integraatiota
- Duplikaatti CSV-headerit: hiljaisesti ylikirjoitetaan — pitäisi hylätä
- Tyhjät header-nimet hyväksytään
- CSP puuttuu `base-uri 'none'`, `frame-ancestors 'none'`, `form-action 'none'`
- Yksittäisen solun kokoa ei rajoiteta

---

## What's Solid

- Moduulirakenne erinomainen: puhtaat, testattavat moduulit ilman framework-riippuvuuksia
- Spec schema kattaa kaikki section-tyypit (chart, table, stats, list) siisteillä Rust-tyypeillä
- Validointisäännöt kattavat merkittävimmät virhetilat
- Testikata laaja (95 unit + 15 integration)
- Turvallisuusmoduulit erotettu puhtaasti

---

## Moderaattorin yhteenveto

**Codex** oli vahvempi: systemaattisempi, löysi enemmän puuttuvia validointeja ja test-aukkoja, oikeat korjausehdotukset (`subtle`, ei `black_box`; reject duplicates, ei auto-suffix).

**Gemini** oli vahvempi XSS-löydöksessä (konkreettinen exploit-string) ja muistiamplifikaation analyysissä.

**Tärkein korjaus:** JSON-parserin datakorruptio (kohta 2) ja XSS-escape (kohta 1) — nämä ovat molemmat silent-failure bugeja joissa virheellinen käytös ei näy mihinkään ennen kuin vahinko on tapahtunut.

**Kumpikaan ei huomannut:** `parse_csv` käyttää `flexible(true)` mutta ei testaa sen vaikutusta ylimääräisiin sarakkeisiin (rivit joissa enemmän kenttiä kuin headerissä hiljaisesti katkaistaan).
