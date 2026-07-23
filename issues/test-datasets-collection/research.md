# Open Source Dataset Research

Research report for issue @intensely-condemned-drum — identifying open source datasets suitable for
testing Glasspad visualization types.

## Datasets Used

### 1. Global CO₂ Emissions (Our World in Data)

- **Source**: [Our World in Data — CO₂ and GHG Emissions](https://github.com/owid/co2-data)
- **License**: CC BY 4.0
- **Format**: CSV, JSON
- **Size**: ~60K rows (full dataset), sampled to 48 rows for test
- **Best for**: Line charts (time series), bar charts (country comparison), stats
- **Description**: Annual CO₂ emissions by country from 1750 to present. We use
  2015-2022 data for the top 6 emitters plus a top-10 snapshot for 2022.

### 2. Iris Flower Dataset (UCI ML Repository)

- **Source**: [UCI Machine Learning Repository](https://archive.ics.uci.edu/dataset/53/iris)
- **License**: CC BY 4.0
- **Format**: CSV
- **Size**: 150 rows × 5 columns (sampled to 45 for test)
- **Best for**: Tables (multi-column numeric data), bar charts (categorical comparison), stats
- **Description**: Classic dataset with sepal/petal measurements for three iris species.
  Widely used benchmark — instantly recognizable to data practitioners.

### 3. World Population (World Bank Open Data)

- **Source**: [World Bank — Population](https://data.worldbank.org/indicator/SP.POP.TOTL)
- **License**: CC BY 4.0
- **Format**: CSV, JSON API
- **Size**: ~16K rows (full), top 15 countries for test
- **Best for**: Bar charts, arc/pie charts, tables, stats
- **Description**: Population by country with demographic indicators (density, urban
  percentage, GDP per capita, life expectancy). Rich multi-column data.

### 4. Titanic Passenger Data (Kaggle)

- **Source**: [Kaggle — Titanic Dataset](https://www.kaggle.com/c/titanic)
- **License**: Public domain
- **Format**: CSV
- **Size**: 891 rows (full), sampled to 31 for test
- **Best for**: Stats (survival aggregates), bar charts (survival by class), arc charts, tables
- **Description**: Passenger survival data from the RMS Titanic. Rich categorical
  fields (class, sex, embarked port) with interesting survival patterns.

### 5. Gapminder Life Expectancy (Gapminder / World Bank)

- **Source**: [Gapminder](https://www.gapminder.org/data/) / [World Bank](https://data.worldbank.org/indicator/SP.DYN.LE00.IN)
- **License**: CC BY 4.0
- **Format**: CSV, JSON
- **Size**: ~10K rows (full), 42 rows for test (6 countries × 7 decades)
- **Best for**: Line charts (long time series), bar charts, tables, stats
- **Description**: Life expectancy, GDP per capita, and population over time.
  Made famous by Hans Rosling's talks. Spans 1960-2020 for diverse countries.

## Datasets Considered but Not Used

| Dataset | Source | Why Not Used |
|---------|--------|--------------|
| MovieLens ratings | GroupLens | Too large, needs preprocessing, no clear viz story |
| NYC Taxi trips | NYC TLC | Massive dataset, hard to sample meaningfully |
| Earthquake data | USGS | Good for maps, but Glasspad doesn't support maps yet |
| Stack Overflow survey | SO | Very wide (100+ columns), unwieldy for inline data |
| Wine Quality | UCI | Overlaps with Iris for the same viz types |
| US Baby Names | data.gov | Interesting for line charts but similar to CO₂/Gapminder |
| Bitcoin prices | Yahoo Finance | Time series only, covered by other datasets |
| Penguin measurements | palmerpenguins | Too similar to Iris |

## Additional Candidates for Future Use

These are worth adding when Glasspad gains new visualization types:

- **Earthquake data** (USGS) — when map sections are added
- **Flight delay data** (Bureau of Transportation) — heatmap/calendar views
- **GitHub commit history** — network/tree visualizations
- **Spotify audio features** — radar/parallel coordinates charts
- **WHO COVID-19 data** — multi-layer time series with annotations

## Coverage Matrix

| Section Type | Open Source Datasets | Fictional Datasets |
|-------------|---------------------|-------------------|
| chart (bar) | co2, iris, population, titanic, gapminder | bug-tracker, restaurant, project-mgmt |
| chart (line) | co2, gapminder | iot-sensors |
| chart (arc) | population, titanic | restaurant |
| table | iris, population, titanic, gapminder | iot-sensors, bug-tracker, restaurant |
| stats | co2, iris, population, titanic, gapminder | project-mgmt, iot-sensors, email, bug-tracker, restaurant |
| list | — | project-mgmt, email, bug-tracker |
| markdown | — | software-architecture |

All Glasspad section types are covered by at least one dataset. The `list` and
`markdown` types are fictional-only since they require domain-specific structured
content that doesn't exist in standard open datasets.
