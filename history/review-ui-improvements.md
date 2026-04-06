# Review: UI Improvements (commits 209d105..62545df)

**Reviewed:** dashboard.js, dashboard.css, schema.rs, analytics-dashboard.yaml
**Reviewers:** Codex (GPT-5.4) — Gemini unavailable (API key expired)
**Rounds:** 1

---

## Must-fix

### 1. `countDistinct()` regressi — `String(v)` conflates types
- `1` ja `"1"` lasketaan samaksi, `true` ja `"true"` samoin
- **Fix:** Palauta `distinctKey(v)` = `typeof v + ':' + String(v)`

### 2. Chart-section piilottaa datalähdevirheet — renderöi tyhjän chartin
- `dataResult.ok ? dataResult.data : []` — jos dataset puuttuu, tyhjä chart
- **Fix:** Tarkista `dataResult.ok` ensin, näytä virhe kuten table/stats

### 3. Table wrapper aina `collapsed` vaikka toggle puuttuu
- Pienet taulukot (<10 riviä) saavat `collapsed`-luokan mutta ei "show more" -nappia
- **Fix:** Lisää `collapsed` vain kun `totalRows > INITIAL_ROWS`

### 4. `isHorizontalBar()` ei tunnista object-muotoista mark:ia
- `cfg.mark !== 'bar'` palauttaa true jos mark on `{ type: "bar", ... }`
- **Fix:** Käytä `getMarkType(mark)` apufunktiota

### 5. Schema `sort` on `Option<String>` ilman validointia
- Kirjoitusvirhe (`sort: temproal`) ohitetaan hiljaisesti → string-sort
- **Fix:** Enum `SortType { Number, String, Temporal, Boolean }`

### 6. Tooltip ei lisäydy object-muotoisiin markeihin
- `typeof cfg.mark === 'string'` tarkistus ohittaa valmiit objektit
- **Fix:** `normalizeMark()` joka lisää `tooltip: true` jos puuttuu

---

## Should-fix

### 7. Temporal sort käyttää string-vertailua
- Toimii ISO-8601:lle mutta hajoaa locale/epäyhtenäisillä formaateilla
- **Fix:** `Date.parse()` fallbackina

### 8. Numeerinen sort coercee virheelliset arvot nollaksi
- `(Number(a) || 0)` — "abc" → 0
- **Fix:** Explicit `isFinite` check, non-numerics loppuun

### 9. Taulukon koko rebuild joka sortilla
- Tuhoaa DOM-tilan, vaikeuttaa tulevaa suodatusta
- **Fix:** Rakenna thead kerran, päivitä vain tbody

### 10. Header-wrapping hauras (addCollapseToggle muokkaa DOM:ia)
- **Fix:** Rakenna section-header vakiorakenteeksi renderSection:ssa

### 11. Sortable th:t eivät ole accessible
- Ei `aria-sort`, ei keyboard focus, ei button-elementtiä
- **Fix:** Button th:n sisään, `aria-sort` attribuutti

### 12. Collapsed chart estää x-scrollin
- `overflow: hidden` poistaa myös horisontaalisen scrollin
- **Fix:** `overflow-x: auto; overflow-y: hidden;`

---

## Minor

- ↕ sort-indikaattori jokaisessa sarakkeessa on visuaalista melua — näytä vain hover/active
- `scrollIntoView({ block: 'nearest' })` → `block: 'start'` selkeämpi
- Taulukon solut ellipsoituvat ilman `title`-attribuuttia → lisää title
- `.table-row-count` CSS on kuollut — poista
- `align-items: baseline` section-headerissa hauras multiline-otsikoille
- Duplikaatti category-skannaus renderissä + span-päätöksessä

---

## Moderaattorin yhteenveto

Löydökset 1-4 ovat oikeita bugeja jotka pitää korjata. Löydös 5 (sort enum) on hyvä laatuparannus. Löydökset 7-12 ovat tärkeitä ennen suodatusvaihetta.

**Tärkein yksittäinen asia:** Section-rakenteen vakiointi (header+body+actions) ennen suodatusvaihetta — nykyinen DOM-muokkaus on hauras ja monimutkaistuu joka lisäyksellä.
