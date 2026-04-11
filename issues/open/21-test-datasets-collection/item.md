---
created: 2026-04-11
updated: 2026-04-11
type: task
reporter: jari
assignee: jari
status: open
priority: normal
---

# 21. Test datasets collection

_Source: test data_

## Description

Curate a collection of test datasets — both fictional and open source — for testing and demonstrating all Glasspad visualization types. Each dataset should exercise different features and content types.

## Goals

1. Identify and collect open source datasets from the web
2. Design additional fictional datasets for scenarios not covered by open data
3. Ensure every Glasspad section type and feature has at least one good test dataset

## Open Source Dataset Sources to Research

- **Kaggle** — datasets across all domains
- **data.gov** / **data.europa.eu** — government open data
- **UCI Machine Learning Repository** — classic structured datasets
- **GitHub awesome-datasets** lists
- **Our World in Data** — global statistics
- **World Bank Open Data** — economic indicators
- **FiveThirtyEight** — journalism datasets

## Datasets Needed by Visualization Type

| Visualization | Ideal dataset characteristics |
|---------------|-------------------------------|
| **Charts** (bar, line, area) | Time series, categorical comparisons |
| **Tables** | Tabular data with many columns, sorting/filtering |
| **Pivot tables** | Multi-dimensional data with numeric values to aggregate |
| **Kanban board** | Items with status/category field and rich detail |
| **Markdown** | Documentation, articles, reports |
| **Diagrams** (Mermaid) | Architecture docs, process flows |
| **Email** | Email-like message data with headers, body, attachments |

## Fictional Dataset Ideas

- **Project management** — tasks, sprints, team members, story points (kanban)
- **IoT sensor data** — time series from multiple sensors (charts)
- **Restaurant menu + orders** — menu items, daily orders, ratings (pivot, charts)
- **Student grades** — courses, students, assignments, scores (pivot, tables)
- **Bug tracker** — issues with status, priority, assignee, tags (kanban, charts)
- **Weather station** — multi-city temperature, humidity, wind over time (charts)

## Acceptance Criteria

- [ ] Research report: list of suitable open source datasets with URLs and descriptions
- [ ] At least 5 open source datasets downloaded/converted to Glasspad-compatible JSON
- [ ] At least 3 fictional datasets created for specific visualization scenarios
- [ ] Each Glasspad section type has at least one dedicated test dataset
