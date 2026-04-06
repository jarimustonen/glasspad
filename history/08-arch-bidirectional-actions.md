# Arkkitehtuuri: Kaksisuuntaiset toiminnot (pad → agent)

## Edellytys

Rikkaat datanäkymät (07). Toiminnot ovat luonnollinen jatke: käyttäjä
ei vain selaa dataa vaan tekee päätöksiä.

## Konsepti

Sectionit voivat sisältää **toimintopainikkeita** (actions). Käyttäjä tekee
valintoja selaimessa ja klikkaa "Done". CLI-komento joka loi padin **odottaa
blokkaten** ja palauttaa kaikki toiminnot JSONina agentille.

Tämä on kuin `$EDITOR` tai `read` — agentti delegoi kontrollin käyttäjälle
ja saa tuloksen takaisin.

## Blocking-malli (CLI ↔ selain)

```
Agentti                         Selain                      Serveri
  │                               │                           │
  │ glasspad create --wait        │                           │
  │──────────────────────────────────────────────────────────→│
  │  (blokkaa, odottaa)           │                           │
  │                               │  GET /:id                 │
  │                               │──────────────────────────→│
  │                               │  ← renderöity HTML        │
  │                               │                           │
  │                               │  (käyttäjä tekee valintoja)
  │                               │  POST events: archive msg-42
  │                               │  POST events: delete msg-17
  │                               │                           │
  │                               │  klikkaa [Done]           │
  │                               │  POST /api/pads/:id/done  │
  │                               │──────────────────────────→│
  │                               │                           │
  │  ← stdout: JSON               │                           │
  │  (blokkaus päättyy)           │                           │
  │                               │                           │
  │  (agentti käsittelee)         │                           │
```

### CLI

```bash
# Luo pad ja odota käyttäjän toimintoja
glasspad create --file inbox.yaml --data emails=messages.json --wait
# → Created pad abc123
# → http://localhost:3000/abc123
# → Waiting for user...
#
# (käyttäjä tekee valintoja selaimessa, klikkaa "Done")
#
# stdout (JSON):
# {"pad_id":"abc123","actions":[
#   {"action":"archive","item":{"id":"msg-42","subject":"Re: Q1 Budget"}},
#   {"action":"delete","item":{"id":"msg-17","subject":"Weekly standup"}}
# ]}

# Ilman --wait: luo padin ja palaa heti (nykyinen käytös)
glasspad create --file inbox.yaml --data emails=messages.json
```

`--wait` blokkaa polling-loopilla: `GET /api/pads/:id/done` kunnes
serveri vastaa 200 (käyttäjä on klikannut Done).

### Agenttisessä loopissa

Agentti (Claude Code, OpenClaw) kutsuu `glasspad create --wait` Bash-toolilla.
Tool-kutsu blokkaa kunnes käyttäjä on valmis. Agentin ei tarvitse pollata —
se saa tuloksen yhdellä kutsulla:

```
Agentti: "Tässä on sähköpostisi. Avaa http://localhost:3000/abc123 ja
         merkitse viestit joita haluat käsitellä. Klikkaa Done kun olet valmis."

         [Bash: glasspad create --wait --file inbox.yaml --data emails=msgs.json]

         (odottaa...)

         (käyttäjä tekee valintoja, klikkaa Done)

         → saa JSON-tuloksen → käsittelee toiminnot
```

## YAML-spec

```yaml
sections:
  - title: "Inbox"
    type: list
    source: emails
    list:
      title_field: subject
      subtitle_field: from
      detail:
        body_field: body_html
        actions:                            # ← toiminnot detail-näkymässä
          - { id: archive, label: "Archive", style: secondary }
          - { id: delete, label: "Delete", style: danger }
          - { id: reply, label: "Reply", style: primary }
          - { id: flag, label: "Flag", style: outline }

  - title: "Review items"
    type: table
    source: reviews
    row_actions:                             # ← toiminnot taulukon riveillä
      - { id: approve, label: "✓", style: success }
      - { id: reject, label: "✗", style: danger }
```

## Done-painike

Kun padissa on toimintoja (`actions`, `row_actions`, `batch_actions`),
renderöidään kelluva "Done"-painike:

```
┌──────────────────────────────────────────┐
│  Inbox                                   │
│  ☑ msg-42: Archive                       │
│  ☑ msg-17: Delete                        │
│  ...                                     │
│                                          │
│       [ Done — send 4 actions ]          │  ← vapauttaa CLI:n
└──────────────────────────────────────────┘
```

- Painikkeessa näkyy toimintojen lukumäärä
- Painike on aktiivinen vain kun toimintoja on tehty
- Klikkaus lähettää `POST /api/pads/:id/done` → serveri merkitsee padin valmiiksi
- CLI huomaa tämän ja tulostaa toiminnot JSON-muodossa stdouttiin

## Event queue (serverin sisäinen)

Toiminnot kerääntyvät event queueen sitä mukaa kun käyttäjä tekee valintoja:

```
POST /api/pads/:id/events
{
  "type": "action",
  "action": "archive",
  "item": { "id": "msg-42", "subject": "Re: Q1 Budget", "from": "maria@..." },
  "timestamp": "2026-04-06T12:00:00Z"
}
```

Done-nappi triggeröi:
```
POST /api/pads/:id/done
→ 200 OK
```

CLI lukee kertyneet eventit:
```
GET /api/pads/:id/events
→ [{ "action": "archive", "item": {...} }, ...]
```

## Visuaalinen palaute

Kun käyttäjä klikkaa toimintopainiketta:

1. Painike näyttää lyhyen vahvistuksen (checkmark)
2. Kohde merkitään visuaalisesti käsitellyksi (fade, yliviivaus, badge)
3. Done-painikkeen laskuri kasvaa

```yaml
    list:
      on_action: fade            # fade | hide | badge | none
```

## Batch-toiminnot

Listassa ja taulukossa voi olla checkbox-valinta:

```yaml
  - title: "Inbox"
    type: list
    source: emails
    selectable: true                    # ← checkboxit
    batch_actions:
      - { id: archive_all, label: "Archive selected" }
      - { id: delete_all, label: "Delete selected" }
```

Batch-toiminto tuottaa yhden eventin jossa kaikki valitut kohteet:
```json
{
  "type": "batch_action",
  "action": "archive_all",
  "items": [
    { "id": "msg-42", "subject": "..." },
    { "id": "msg-17", "subject": "..." }
  ]
}
```

## Agentin työnkulku

```
1. Agentti kerää dataa (esim. lukee sähköpostit)
2. glasspad create --file inbox.yaml --data emails=messages.json --wait
   → "Created pad abc123, http://localhost:3000/abc123"
   → "Waiting for user..."
   (blokkaa)
3. Käyttäjä avaa dashboardin, selaa viestejä, merkitsee toimintoja
4. Käyttäjä klikkaa "Done"
5. CLI palauttaa JSON:n stdouttiin, blokkaus päättyy
6. Agentti parsii JSON:n, suorittaa toiminnot
7. Voi luoda uuden padin päivitetyllä datalla (loop)
```

## Vaikutus arkkitehtuuriin

**Serveri**:
- Uusi endpoint: `POST /api/pads/:id/events` (selain → serveri, kerää toiminnot)
- Uusi endpoint: `POST /api/pads/:id/done` (selain ilmoittaa valmis)
- Uusi endpoint: `GET /api/pads/:id/done` (CLI pollaa onko valmis)
- Uusi endpoint: `GET /api/pads/:id/events` (CLI lukee kertyneet toiminnot)
- Event storage: Vec<Event> per pad (in-memory)
- Done-tila: bool per pad

**YAML-spec**: uudet kentät `actions`, `row_actions`, `batch_actions`, `selectable`

**Client-side**: action-painikkeet, Done-painike, POST-kutsut, visuaalinen palaute

**CLI**: `--wait` lippu `create`-komennolle

## Tulevaisuus: OpenClaw-integraatio

Myöhemmin glasspad voisi toimia OpenClaw-päätelaitteena (terminal),
jolla on oma pitkäikäinen sessio. Tässä mallissa:

- Glasspad on oma OpenClaw-plugin joka rekisteröi päätelaitteen
- Toiminnot tulevat kontekstina seuraavaan turn-pyyntöön (ei pollausta)
- Sessio säilyy padien välillä — agentti voi ylläpitää jatkuvaa näkymää
- Mahdollistaa reaaliaikaisemman vuorovaikutuksen kuin CLI-blocking-malli

Tämä on erillinen suunnitteluvaihe joka ei vaikuta CLI-mallin toteutukseen.
