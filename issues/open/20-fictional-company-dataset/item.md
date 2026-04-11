---
created: 2026-04-11
updated: 2026-04-11
type: task
reporter: jari
assignee: jari
status: open
priority: normal
---

# 20. Fictional company financial dataset

_Source: test data_

## Description

Create a comprehensive fictional company dataset that simulates realistic business financial data. The dataset should be rich enough to exercise all Glasspad visualization types: tables, charts, pivot tables, kanban boards, etc.

## Company Profile

A fictional mid-size company (e.g. "NorthWave Solutions Oy" — a Nordic tech/SaaS company) with:

- 3-5 years of historical data
- Multiple departments, products, and regions
- Realistic seasonal patterns and growth trends

## Dataset Components

### Accounting / Bookkeeping
- **General ledger** — journal entries with account codes, dates, descriptions, debit/credit amounts
- **Chart of accounts** — account hierarchy (assets, liabilities, equity, revenue, expenses)
- **Accounts payable** — vendor invoices, due dates, payment status
- **Accounts receivable** — customer invoices, amounts, aging

### Financial Statements
- **Income statement** (P&L) — monthly/quarterly revenue, COGS, operating expenses, net income
- **Balance sheet** — assets, liabilities, equity snapshots
- **Cash flow statement** — operating, investing, financing activities

### Sales & Revenue
- **Sales transactions** — date, customer, product, quantity, unit price, total, salesperson
- **Customer list** — company name, segment, region, acquisition date
- **Product catalog** — product name, category, unit cost, list price

### HR & Payroll
- **Employee roster** — name, department, role, hire date, salary
- **Payroll records** — monthly salary, taxes, benefits, net pay

### Operations
- **Purchase orders** — vendor, items, quantities, costs
- **Inventory** — product stock levels, reorder points
- **Projects/tasks** — project name, status, budget, actual spend, deadlines

## Format

JSON datasets suitable for Glasspad API consumption. Each component as a separate dataset that can be cross-referenced (shared keys like customer_id, product_id, employee_id).

## Acceptance Criteria

- [ ] Internally consistent data (ledger balances, P&L ties to ledger, etc.)
- [ ] Realistic patterns (seasonality, growth, variance)
- [ ] Enough volume for meaningful aggregation (~1000+ transactions)
- [ ] Works with pivot tables, charts, tables, and kanban views
