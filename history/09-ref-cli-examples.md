# Referenssi: CLI-esimerkit

## Peruskäyttö (nykyinen, toimii jo)

```bash
# Käynnistä serveri (tapahtuu automaattisesti)
glasspad serve

# Luo pad YAML-tiedostosta
glasspad create --file dashboard.yaml

# Luo pad stdinistä
cat <<'YAML' | glasspad create
title: "Quick Stats"
sections:
  - title: "Numbers"
    type: stats
    data:
      - { label: "Users", value: 1234 }
YAML

# Listaa padit
glasspad list

# Avaa selaimessa
glasspad open abc123

# Dokumentaatio
glasspad docs
glasspad docs spec
glasspad docs charts
glasspad docs examples
glasspad docs api

# Skill-asennus
glasspad skill --install-claude
glasspad skill --install-claude --user
```

## Ulkoiset datalähteet (vaihe 2)

```bash
# Dashboard + CSV-data
glasspad create --file analytics.yaml --data events=events.csv

# Useita datalähteitä
glasspad create --file overview.yaml \
  --data events=events.csv \
  --data users=users.json \
  --data revenue=revenue.csv

# Päivitä vain data (spec pysyy samana)
glasspad update abc123 --data events=updated-events.csv

# Päivitä spec ja data
glasspad update abc123 --file new-spec.yaml --data events=new-data.csv
```

## Interaktiiviset dashboardit (vaihe 3)

```bash
# Agentti luo interaktiivisen analytiikka-dashboardin
glasspad create --file analytics.yaml --data events=events.csv
# → Created pad abc123
# → http://localhost:3000/abc123
# Käyttäjä avaa selaimessa, klikkaa charteissa suodattaakseen

# Agentti luo sähköpostinäkymän
glasspad create --file inbox.yaml --data emails=messages.json
# → Created pad def456
# → http://localhost:3000/def456
# Käyttäjä selaa viestejä, avaa yksittäisiä
```

## Kaksisuuntaiset toiminnot (vaihe 4)

```bash
# Agentti luo inbox-dashboardin ja JÄÄ ODOTTAMAAN käyttäjän toimintoja
glasspad create --file inbox.yaml --data emails=messages.json --wait
# → Created pad def456
# → http://localhost:3000/def456
# → Waiting for user...
#
# (käyttäjä tekee valintoja selaimessa, klikkaa "Done")
#
# → stdout JSON:
# {"pad_id":"def456","actions":[
#   {"action":"archive","item":{"id":"msg-42","subject":"Re: Q1 Budget"}},
#   {"action":"delete","item":{"id":"msg-17","subject":"Weekly standup"}}
# ]}

# Ilman --wait: luo padin ja palaa heti (nykyinen käytös)
glasspad create --file inbox.yaml --data emails=messages.json
```

## Tyypillinen agenttisessio: read-only dashboard

```bash
# 1. Agentti kerää dataa
git log --format='%H,%an,%s,%aI' > /tmp/commits.csv

# 2. Agentti luo dashboardin (ei --wait, palaa heti)
glasspad create --file /tmp/git-dashboard.yaml --data commits=/tmp/commits.csv
# → Created pad abc123
# → http://localhost:3000/abc123

# 3. Käyttäjä avaa, klikkaa, suodattaa selaimessa
# 4. Agentti voi myöhemmin päivittää
glasspad update abc123 --data commits=/tmp/fresh-commits.csv
```

## Tyypillinen agenttisessio: interaktiivinen toimintalooppi

```bash
# 1. Agentti kerää sähköpostit
fetch-emails --format json > /tmp/inbox.json

# 2. Agentti luo näkymän ja odottaa (blokkaa)
RESULT=$(glasspad create --file inbox.yaml --data emails=/tmp/inbox.json --wait)
# → käyttäjä tekee valintoja selaimessa...
# → klikkaa Done
# → RESULT sisältää JSON:n toiminnoista

# 3. Agentti käsittelee toiminnot
echo "$RESULT" | process-email-actions

# 4. Voi toistaa loopin päivitetyllä datalla
```
