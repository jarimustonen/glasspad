# Test Datasets

Test datasets for validating and demonstrating all Glasspad visualization types.

Each `.yaml` file is a self-contained Glasspad dashboard spec with inline data.
Use with: `curl -X POST http://localhost:3000/api/pads -H "Content-Type: application/x-yaml" -d @<file>.yaml`

## Open Source Datasets

| File | Source | Visualization Types |
|------|--------|-------------------|
| `global-co2-emissions.yaml` | Our World in Data | line chart, bar chart, stats, table |
| `iris-flowers.yaml` | UCI ML Repository | table, bar chart, stats |
| `world-population.yaml` | World Bank | bar chart, arc chart, stats, table |
| `titanic-survival.yaml` | Kaggle | stats, bar chart, arc chart, table |
| `gapminder-life-expectancy.yaml` | Gapminder / World Bank | line chart, table, stats |

## Fictional Datasets

| File | Scenario | Visualization Types |
|------|----------|-------------------|
| `project-management.yaml` | Sprint task board | list (kanban-style), stats, bar chart |
| `iot-sensor-readings.yaml` | Factory sensor monitoring | line chart, stats, table |
| `email-inbox.yaml` | Corporate email inbox | list (detail view), stats |
| `software-architecture.yaml` | System design docs | markdown (with Mermaid diagrams) |
| `bug-tracker.yaml` | Issue tracking system | list, stats, bar chart, table |
| `restaurant-orders.yaml` | Restaurant analytics | bar chart, arc chart, stats, table |

## Section Type Coverage

| Section Type | Datasets |
|-------------|----------|
| **chart** (bar) | global-co2, iris, world-population, titanic, bug-tracker, restaurant-orders |
| **chart** (line) | global-co2, gapminder, iot-sensor-readings |
| **chart** (arc) | world-population, titanic, restaurant-orders |
| **table** | iris, world-population, titanic, gapminder, iot-sensor-readings, bug-tracker, restaurant-orders |
| **stats** | global-co2, iris, titanic, project-management, iot-sensor-readings, bug-tracker, restaurant-orders |
| **list** | project-management, email-inbox, bug-tracker |
| **markdown** | software-architecture |
