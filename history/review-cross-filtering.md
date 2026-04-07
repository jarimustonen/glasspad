# Review: Interactive Cross-Filtering (commit e620118)

**Reviewed:** src/client/dashboard.js, src/client/dashboard.css
**Reviewers:** Gemini (gemini-3.1-pro-preview), Codex (GPT-5.4)
**Rounds:** 2

---

## Critical Issues (Consensus)

### 1. Falsy values cannot be un-toggled

- **What:** `if (fieldFilters[field][key])` fails for values `0`, `""`, `false`. Toggle becomes one-way — can add but never remove.
- **Where:** `toggleFilter()` ja myös `isValueFiltered()`
- **Fix:** `if (key in fieldFilters[field])` tai `hasOwnProperty`

### 2. Repeated dataset filtering + O(N) lookups

- **What:** `getFilteredData()` kutsutaan erikseen jokaisessa sectionissa per filter-muutos. Object.keys() + lineaarinen haku per rivi.
- **Fix:** Memoize filteredCache per onFilterChange-sykli + suora `key in allowed` -tarkistus

### 3. Table/chart collapse state hajoaa suodatuksella

- **What:** Collapse-kontrollit luodaan kerran staattisilla teksteillä. Suodatus muuttaa rivimäärää/kategorioita mutta toggle-nappi näyttää vanhan luvun, ei piilotu/palaudu oikein.
- **Fix:** `addCollapseToggle` palauttaa controllerin jolla voi päivittää tekstin ja näkyvyyden. Table rebuild kutsuu sitä.

### 4. Chart cursor CSS/JS ristiriita

- **What:** JS asettaa `div.style.cursor` sisäiseen diviin, CSS kohdistuu `.chart-container[style*="cursor"]`. Eivät kohtaa.
- **Gemini:** Käytä Vegan `mark.cursor = 'pointer'` — kohdistuu vain data-elementteihin
- **Codex:** Käytä luokkaa `.interactive-chart`
- **Fix:** Geminin ratkaisu parempi — pointer vain palkeissa/kaarioissa, ei tyhjässä tilassa

### 5. Vega embed async race

- **What:** `vegaEmbed` on asynkroninen. Jos filter muuttuu ennen resolvea, `chartViews[key]` on undefined → chart jää suodattamatta.
- **Severity:** Lievä — seuraava filter-muutos korjaa tilanteen. Mutta ensimmäinen klikkaus voi jäädä huomaamatta.
- **Fix:** `.then()` callbackissa sovella nykyinen filter-tila

---

## Disputed Issues

### 6. Ei visuaalista palautetta valituille arvoille charteissa

- **Gemini:** Kriittinen UX-ongelma. Pitäisi himmentää ei-valitut palkit (opacity).
- **Codex:** Samaa mieltä, mutta implementation vaatii Vega signal -integraatiota joka on monimutkainen.
- **Moderaattori:** Molemmat oikeassa. Tärkeä UX-parannus mutta voi olla vaihe 2. Filter bar antaa jonkin verran palautetta jo nyt.

### 7. Object-as-set prototyyppiriski

- **Codex:** `filterState[source]` ja `fieldFilters[field]` voivat osua prototyypin avaimiin. Käytä `Object.create(null)`.
- **Gemini:** `distinctKey()` suojaa arvoja (`"string:__proto__"`), ei todellista riskiä.
- **Moderaattori:** Gemini oikeassa data-avaimista. Mutta `source` ja `field` -avaimet tulevat specistä — `Object.create(null)` on silti hyvä käytäntö.

---

## Minor Issues

- Dead code: `getFilteredDataResult()`, `isValueFiltered()` — poista
- Filter bar tagit ilman dataset-nimeä — epäselvä jos useita datasettejä
- Chart collapse-korkeus lasketaan suodattamattomasta datasta, ei päivity
- Filter bar pulse triggeröityy myös poistossa — pitäisi vain lisäyksessä
- `mountStats` muutti fallback-semantiikkaa (inline_data ei enää toimi fallbackina)
- Ei interactive_filter -validointia mount-vaiheessa

---

## Moderaattorin yhteenveto

**Gemini** löysi kriittisimmän bugin (falsy toggle) ja async racen. **Codex** oli systemaattisempi ja löysi enemmän arkkitehtuurisia ongelmia (collapse state, cursor, dead code, prototype).

**Tärkein korjaus:** Falsy toggle bug (#1) ja filtered cache (#2) — molemmat ovat helppoja korjata ja vaikuttavat suoraan käyttökokemukseen.

**Seuraava UX-parannus:** Visuaalinen palaute valituille arvoille charteissa (himmennetyt palkit) — tämä tekee suodatuksesta paljon intuitiivisemman.
