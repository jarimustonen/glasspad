# Plan: Filter Redesign — Explicit Selection Mode

## Motivaatio

Nykyinen "quick filter" (klikkaa palkkia → heti suodattaa) on ongelmallinen:
- Yksi klikkaus = yksi arvo = kaikki muu katoaa
- Ei voi valita useita arvoja kerralla (esim. IN + US + SG)
- Käyttäjä ei tiedä etukäteen mitä klikkaus tekee

## Uusi malli: kaksi tilaa

### Näyttötila (oletus)
- Chart näyttää datan normaalisti
- Header-palkissa filter-ikoni (🔍) jos section on `interactive_filter`
- Ikoni on neutraali kun ei suodatusta, korostettu kun suodatus aktiivinen

### Suodatustila
- Aktivoituu klikkaamalla filter-ikonia
- Kaikki palkit/sektorit oletuksena valittuja (värikkäitä)
- Klikkaa palkkia → toggle: valittu (värikäs) / ei-valittu (himmeä, opacity ~0.3)
- Voi klikata useita
- Headerissa "Apply" ja "Cancel" -napit (korvaa filter-ikonin)
- "Apply" → suodatus aktivoituu, palaa näyttötilaan
- "Cancel" → ei muutoksia, palaa näyttötilaan

### Näyttötila suodatettuna
- Data suodatettu valintojen mukaan kaikkiin saman datasetin sectioneihin
- Filter-ikoni korostettu (sininen/aktiivinen)
- Klikkaa ikonia → palaa suodatustilaan muokkaamaan
- Filter bar ylhäällä näyttää aktiiviset suodattimet

## Visuaalinen suunnittelu

### Section header suodatustilassa
```
┌─────────────────────────────────────────────────────────┐
│  BY COUNTRY                        [Cancel]  [Apply ✓]  │
│                                                         │
│  ██████████████████████ SG   ← värikäs (valittu)       │
│  ████████████████████   IN   ← värikäs (valittu)       │
│  ░░░░░░░░░░░           US   ← himmeä (ei valittu)     │
│  ░░░░░░░░              OM   ← himmeä (ei valittu)     │
│  ...                                                    │
└─────────────────────────────────────────────────────────┘
```

### Section header näyttötilassa (suodatus aktiivinen)
```
┌─────────────────────────────────────────────────────────┐
│  BY COUNTRY                                 [🔍 active] │
│                                                         │
│  ██████████████████████ SG                              │
│  ████████████████████   IN                              │
│  (vain valitut maat näkyvissä)                          │
└─────────────────────────────────────────────────────────┘
```

### Filter bar (ennallaan mutta päivitetty)
```
┌─────────────────────────────────────────────────────────┐
│  Filters: [events · country: SG, IN ×]     [Reset all]  │
└─────────────────────────────────────────────────────────┘
```

## Tekninen toteutus

### 1. Poista quick filter
- Poista suora `view.addEventListener('click', toggleFilter)` chartista
- Klikkaus ei enää suodaa suoraan

### 2. Filter-ikoni section headeriin
- `mountChart`: jos `interactive_filter` → lisää filter-nappi `s.actions`:iin
- Nappi: 🔍 ikoni, pieni, neutraali harmaana
- Aktiivinen: sininen korostus kun suodatus päällä

### 3. Suodatustila
- Klikkaa filter-nappia → section siirtyy suodatustilaan
- `pendingSelection`: Object (key → true/false) — mitkä arvot valittu
- Oletuksena kaikki valittu (tai jos suodatus jo aktiivinen, nykyinen valinta)
- Vega-Lite: lisää opacity-encoding joka himmentää ei-valitut
  - `opacity: { condition: { test: "...", value: 1 }, value: 0.3 }`
  - Tai yksinkertaisemmin: lisää `_selected` kenttä dataan ja käytä sitä
- Klikkaus vaihtaa `pendingSelection[value]` ja päivittää chartin

### 4. Apply / Cancel
- Apply: kopioi `pendingSelection` → `filterState`, kutsu `onFilterChange()`
- Cancel: hylkää `pendingSelection`, palauta alkuperäinen chart
- Molemmat palauttavat näyttötilaan

### 5. Datan enrichment suodatustilassa
Yksinkertaisin tapa: lisää `_selected: true/false` kenttä jokaiseen datariviin
ja käytä sitä opacity-encodingissa:

```javascript
// Suodatustilassa:
var enriched = rawData.map(function(row) {
  var copy = Object.assign({}, row);
  copy._selected = pendingSelection[distinctKey(row[filterField])] !== false;
  return copy;
});
view.data('source', enriched).run();
```

Vega-Lite spec saa opacity-encodingin:
```javascript
vlSpec.encoding.opacity = {
  condition: { field: '_selected', value: 1 },
  value: 0.3
};
```

### 6. Filter state -muutos
- `filterState[source][field]` sisältää nyt **valitut arvot** (include-lista)
- Jos kenttä ei ole filterStatessa → kaikki näytetään
- `getFilteredData` pysyy ennallaan (vain valitut läpäisevät)

## Tilat per section

```
sectionFilterState = {
  mode: 'view' | 'edit',
  pendingSelection: { 'string:IN': true, 'string:US': false, ... } | null
}
```

## Poistettavat asiat
- Quick filter (suora klikkaus → suodatus)
- Mahdollisesti pulse-animaatio filter barista (ei enää "yllätys"-suodatuksia)

## Säilyvät asiat
- Filter bar tageineen ja reset-napilla
- Filter state per dataset, per field
- Memoized filteredCache
- Table/stats/chart update registry
- Collapse toggle
