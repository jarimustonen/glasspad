# NorthWave Solutions Oy — Fictional Company Dataset

Comprehensive financial dataset for a fictional Nordic SaaS company, spanning
January 2023 to December 2025 (3 years). All monetary values in EUR.

## Company Profile

- **Name:** NorthWave Solutions Oy
- **Industry:** B2B SaaS (analytics & data integration)
- **HQ:** Helsinki, Finland
- **Founded:** 2021
- **Employees:** ~28 (growing from ~10 to ~27 over the period)
- **Revenue trajectory:** ~272k (2023) → ~790k (2024) → ~1.18M (2025)
- **Stage:** Growth-stage startup, pre-profitability

## Files (6,304 total records)

| File | Records | Description |
|------|---------|-------------|
| `chart_of_accounts.json` | 31 | Account hierarchy: assets, liabilities, equity, revenue, expenses |
| `products.json` | 8 | Product catalog with cost/price, categories |
| `customers.json` | 25 | Customer list with segment, region, acquisition date |
| `employees.json` | 28 | Employee roster with department, role, salary, hire date |
| `vendors.json` | 10 | Vendor list with payment terms |
| `sales_transactions.json` | 1,553 | Individual sales with customer, product, quantity, discounts |
| `accounts_receivable.json` | 1,553 | Customer invoices with aging and payment status |
| `accounts_payable.json` | 280 | Vendor invoices with due dates and payment status |
| `general_ledger.json` | 1,732 | Double-entry journal entries (balanced: debits = credits) |
| `payroll.json` | 781 | Monthly payroll with tax, benefits, employer contributions |
| `income_statement.json` | 36 | Monthly P&L with revenue breakdown and expense categories |
| `balance_sheet.json` | 12 | Quarterly balance sheet snapshots |
| `projects.json` | 12 | Company projects with budget, status, timeline |
| `tasks.json` | 27 | Project tasks with status, assignee, hours |
| `inventory.json` | 216 | Monthly subscription/seat counts per product |

## Cross-reference Keys

All files share consistent IDs for joins:

- `customer_id` (CUST-NNN) — customers ↔ sales ↔ AR
- `product_id` (PROD-NNN) — products ↔ sales ↔ inventory
- `employee_id` (EMP-NNN) — employees ↔ payroll ↔ tasks (as salesperson/assignee)
- `vendor_id` (VEND-NNN) — vendors ↔ AP
- `account_code` (NNNN) — chart of accounts ↔ general ledger
- `project_id` (PROJ-NNN) — projects ↔ tasks
- `sale_id` / `invoice_id` — sales ↔ AR

## Visualization Suitability

| View Type | Best Datasets | Example Use |
|-----------|--------------|-------------|
| **Table** | Any file | Sales ledger, employee roster, AR aging |
| **Line chart** | income_statement, inventory | Revenue over time, MRR trends |
| **Bar chart** | sales_transactions, payroll | Revenue by region, department costs |
| **Pie/arc chart** | sales_transactions, income_statement | Revenue by product category, expense breakdown |
| **Pivot table** | sales_transactions, payroll, general_ledger | Sales by region×product, payroll by dept×month |
| **Kanban board** | tasks, projects, accounts_receivable | Task workflow, AR collection pipeline |
| **Stats** | income_statement, sales_transactions | KPIs: total revenue, avg deal size, headcount |

## Data Characteristics

- **Seasonality:** Q1 strong start, summer dip (Jul-Aug), Q4 year-end push
- **Growth:** ~30% YoY revenue growth, accelerating customer acquisition
- **Realistic patterns:** Enterprise discounts, payment aging, employee churn (1 departure)
- **Internal consistency:** Journal entries balance (debits = credits), P&L ties to ledger
- **Finnish context:** ~20% income tax, ~25% employer social contributions, EUR currency
