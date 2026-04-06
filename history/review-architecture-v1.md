# Review: Glasspad Architecture v1

**Reviewed:** Architecture documents 05–11, example YAML specs
**Reviewers:** Gemini (gemini-3.1-pro-preview), Codex (GPT-5.4)
**Rounds:** 2 (independent review + cross-review)

---

## Critical Issues (Consensus)

Molemmat arvioijat ovat yksimielisiä näistä ongelmista.

### 1. Turvallisuusmalli on rikki

**Mikä:** Useita päällekkäisiä turvallisuusaukkoja:
- `body_html` renderöidään ilman sanitointia → XSS
- Raw JSON upotetaan `<script>`-tägiin → `</script>` datassa rikkoo sivun
- `file:` YAML-specissä mahdollistaa mielivaltaisen tiedoston lukemisen (`/etc/passwd`)
- Localhost-mutaatioendpointit (`POST /events`, `/done`) ovat autentikoimattomia → toinen välilehti/sivusto voi kutsua niitä
- `url:` datalähde on SSRF-riski

**Miksi kriittinen:** Glasspad käsittelee agentin kautta internetistä tulevaa dataa (sähköpostit, logit, scrapattu sisältö) ja renderöi sen localhostissa. Oletus "localhost = turvallinen" ei pidä.

**Korjaus:**
- HTML-sisältö oletuksena plain text, sanitoitu HTML vain erikseen pyydettäessä
- JSON upotus: käytä `<script type="application/json">` tai fetch-endpoint
- `file:` pois specistä, vain CLI `--data` -lipulla
- `url:` pois specistä kunnes suunniteltu
- Mutaatio-endpointit vaativat pad-kohtaisen salaisuuden (token URL:ssa tai headerissa)
- CSP-headerit

### 2. Spec/schema ei ole vakaa eikä yksiselitteinen

**Mikä:** Kolme eri tapaa liittää data sectioniin:
- `chart.data: [...]` (inline chartissa)
- `section.data: [...]` (inline sectionissa)
- `source: events` + top-level `data: { events: ... }`

Lisäksi top-level `data:` on sama nimi kuin section-tason `data:`. Stats-syntaksi esimerkeissä eroaa referenssidokumentista. Ei section ID:itä, ei row ID:itä.

**Miksi kriittinen:** AI-agentit generoivat YAML-specejä. Löyhä, epäyhtenäinen schema tuottaa virheellisiä specejä jatkuvasti.

**Korjaus:**
- Nimeä top-level `data:` → `datasets:`
- Yksi kanoninen tapa per section-tyyppi (`source:` + `inline_data:`)
- Pakollinen `section.id` interaktiivisille osioille
- Pakollinen `id_field` toimintoja sisältäville listoille/taulukoille
- Formaalinen schema-validointi ja selkeät virheviestit agentille
- `spec_version: 1` tulevien muutosten hallintaan

### 3. `--wait` / completion -protokolla on alimääritelty

**Mikä:** CLI blokkaa ikuisesti ilman timeoutia. Event queue + done-bool ei ole atominen. Ei idempotenttiutta (tupla-klikkaus = duplikaattitoiminnot). Stdout ja stderr sekoitetaan. Ei peruutusmekanismia (selain tai terminaali).

**Miksi kriittinen:** Tämä on silta selaimen, serverin, CLI:n ja agenttiloupin välillä. Jos se on epäluotettava, koko kaksisuuntaisuus hajoaa.

**Korjaus:**
- `--timeout` lippu (oletus esim. 10min)
- Cancel-painike padissa Done-napin rinnalle
- Ctrl-C CLI:ssä palauttaa `{"status":"cancelled"}`
- Stdout vain JSON-data, stderr statusviestit, `--json` lippu
- Idempotenttiys: client-generoitu event/submission ID
- Pad lukitaan completion-jälkeen (409 Completed)

---

## Kiistanalaiset löydökset

Arvioijat ovat eri mieltä näistä.

### 4. Vega-Lite suodatusstrategia

**Gemini:** Re-rendering suodatetulla datalla on virhe. Vega-Liten sisäinen tila (skaalat, akselit) hyppää. Pitäisi käyttää Vega-Liten natiiveja `params`/Selections.

**Codex:** App-tason filter state + full rerender on validi MVP-arkkitehtuuri. Vega-Lite selections eivät hallitse taulukoita, statseja, listoja, Done-painiketta. Skaalahyppy on UX-tradeoff, ei arkkitehtuurivirhe.

**Moderaattorin arvio:** Codex on vahvemmilla. MVP:ssä app-tason filter state on oikea valinta koska se hallitsee kaikkia section-tyyppejä yhtenäisesti. Vega-Lite selections ovat optimointi joka voidaan lisätä myöhemmin charttien sisäiseen tilaan. Skaalahyppy on hyvä tiedostaa mutta ei estä julkaisua.

### 5. Roadmap-järjestys: suodatus (C) vs. toiminnot (E)

**Gemini:** Käännä järjestys A→B→D→E→C. Toiminnot ovat glasspadin ydinarvoa agenteille, suodatus on UX-parannus.

**Codex:** Nykyinen järjestys on ok. Suodatus on pivot-taulukkomallin ydin, ei lisäominaisuus. Toiminnot ovat monimutkaisempia (completion, auth, idempotenttiys) kuin Gemini esittää.

**Moderaattorin arvio:** Molemmat tekevät valideja pointteja. Tärkein oivallus on Codexin huomio: molemmat tarvitsevat schema/security-pohjan (vaihe A0) ensin. Sen jälkeen järjestys on joustava. Suodatus on luonnollisempi jatkumo data layerille.

### 6. Event queue vs. atominen completion

**Gemini:** Event queue on ok, pitää vain deduplikoida backend-puolella.

**Codex:** Event queue + done-bool on race-altis. Parempi: yksi atominen `POST /complete` joka sisältää kaikki toiminnot.

**Moderaattorin arvio:** Molemmat mallit toimivat. Hybridimalli on paras: toiminnot kertyvät event queueen (käyttäjä näkee edistyksen), mutta Done-nappi lähettää finaalisen submission-paketin jonka serveri tallentaa atomisesti. CLI lukee vain finaalisen tuloksen.

### 7. Full rerender -suorituskyky

**Gemini:** Ei ongelma MVP:ssä, premature optimization.

**Codex:** Arkkitehtuurissa pitäisi vähintään varautua inkrementaaliseen renderöintiin.

**Moderaattorin arvio:** Gemini on oikeassa MVP:stä. Mutta Codexin huomio section-tilasta (detail view auki → rerender → detail katoaa) on todellinen ongelma joka tarvitsee ratkaisun jo MVP:ssä.

---

## Tärkeät löydökset (alempi prioriteetti)

### 8. Filter state pitää rajata datasettiin
Molemmat yhtä mieltä. `filterState` ei voi olla globaali kenttänimien mukaan — pitää olla `filterState[source][field]`.

### 9. CSV-tyyppien päättely puuttuu
Molemmat yhtä mieltä. CSV-parser tuottaa merkkijonoja, mutta Vega-Lite tarvitsee numerot. Tarvitaan vähintään automaattinen tyyppiarvaus (numero, boolean, temporal).

### 10. Toimintopayloadit liian isoja
Molemmat yhtä mieltä. Ei lähetetä koko email-body:a action-eventissä. Vain `id` + avainkenttien tiivistelmä.

### 11. Detail-näkymän tilahallinta puuttuu
Codexin löydös, Gemini validoi. Jos käyttäjä on detail-näkymässä ja suodatus muuttuu: mitä tapahtuu? Tarvitaan section-tason tila.

### 12. Stats-schema on dokumentoimaton
Codexin löydös. Analytics-esimerkki käyttää `aggregate: count`, `filter: { event_type: "visit" }`, `aggregate: distinct` — mitään näistä ei ole referenssidokumentissa.

### 13. Update-semantiikka määrittelemätön
Codexin löydös. `glasspad update` kesken interaktiivisen session: mitä tapahtuu suodatuksille, avoimelle detail-näkymälle, toimintojonolle?

### 14. Detail-moodit: vain `replace` MVP:hen
Molemmat yhtä mieltä. Overlay ja fullscreen vaativat lifecycle-logiikkaa (ESC, back, focus trapping). Ei MVP:hen.

---

## Mikä on kunnossa

- Yleinen YAML-first -filosofia (agentti kuvaa, glasspad renderöi)
- Blocking `--wait` -konsepti (oikea malli agentti-loopille, kunhan toteutus on robusti)
- Data + spec -erotus (oikea suunta)
- Localhost-first -rajaus MVP:lle
- Rust + single binary -valinta
- Roadmapin rinnakkaisuusanalyysi (C||D)

---

## Suositellut toimenpiteet ennen toteutusta

### Must-fix (tekee ennen koodausta)

1. **Kirjoita `04-spec-contract.md`:**
   - Kanoninen YAML-schema (`datasets:`, ei `data:`)
   - Normalisointisäännöt legacy-syntaksille
   - Section ID:t, row ID:t
   - Filter state -malli (per dataset)
   - Stats-aggregaattien grammar
   - Validointivirheet agentille
   - `spec_version: 1`

2. **Kirjoita turvallisuusmalli:**
   - Ei `file:` eikä `url:` specissä
   - Sanitoitu HTML, ei raakaa
   - JSON upotus turvallisesti
   - Pad-token mutaatioihin
   - CSV/JSON kokorajoitukset

3. **Tarkenna `--wait` -protokolla:**
   - Timeout + cancel
   - Stdout/stderr -sopimus + `--json`
   - Atominen completion
   - Idempotenttiys

### Should-fix (pian toteutuksen alussa)

4. CSV-tyyppien päättely
5. Section-tilan hallinta (detail view + suodatukset)
6. Update-semantiikka
7. MVP-kokorajoitukset dokumentoituna
8. Esimerkit päivitettyinä uuteen schemaan

---

## Moderaattorin yhteenveto

Kumpikin arvioija toi todellista arvoa:

**Gemini** oli vahvempi turvallisuuslöydöksissä (XSS, path traversal, CSRF) ja Vega-Lite -yksityiskohdissa.

**Codex** oli vahvempi rakenteellisissa ongelmissa (schema-yhtenäisyys, tila-ristiriidat, completion-protokolla, CLI-sopimus) ja tuotti systemaattisemman analyysin.

**Kumpikaan ei huomannut:** Glasspadin "server käynnistyy automaattisesti" -malli (nykyinen toteutus) voi jättää orpo-prosesseja pyörimään taustalle. Tarvitaan PID-tiedosto tai socket-tarkistus.

**Tärkein yksittäinen asia:** Kirjoita kanoninen spec-sopimus (`04-spec-contract.md`) ennen kuin toteutat yhtään uutta ominaisuutta. Ilman sitä jokainen vaihe tuottaa ad hoc -parserilogiikkaa ja yhteensopivuushakkeja.
