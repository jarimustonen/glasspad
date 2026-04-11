#!/usr/bin/env python3
"""Generate fictional company dataset for NorthWave Solutions Oy.

Creates internally consistent financial, sales, HR, and operations data
for a Nordic SaaS company spanning 2023-01 to 2025-12 (3 years).

All monetary values in EUR.
"""

import json
import random
import os
from datetime import date, timedelta
from pathlib import Path

random.seed(42)  # Reproducible

OUT = Path(__file__).parent / "data"
OUT.mkdir(exist_ok=True)

# ---------------------------------------------------------------------------
# Reference data
# ---------------------------------------------------------------------------

DEPARTMENTS = ["Engineering", "Sales", "Marketing", "Finance", "Operations", "HR", "Support"]

PRODUCTS = [
    {"product_id": "PROD-001", "name": "GlassView Analytics", "category": "Analytics", "unit_cost": 120.00, "list_price": 299.00, "type": "subscription"},
    {"product_id": "PROD-002", "name": "GlassView Pro", "category": "Analytics", "unit_cost": 200.00, "list_price": 599.00, "type": "subscription"},
    {"product_id": "PROD-003", "name": "DataStream Connector", "category": "Integration", "unit_cost": 50.00, "list_price": 149.00, "type": "subscription"},
    {"product_id": "PROD-004", "name": "CloudSync Gateway", "category": "Integration", "unit_cost": 80.00, "list_price": 249.00, "type": "subscription"},
    {"product_id": "PROD-005", "name": "NorthWave Platform", "category": "Platform", "unit_cost": 500.00, "list_price": 1499.00, "type": "subscription"},
    {"product_id": "PROD-006", "name": "Setup & Onboarding", "category": "Services", "unit_cost": 800.00, "list_price": 2500.00, "type": "one-time"},
    {"product_id": "PROD-007", "name": "Training Package", "category": "Services", "unit_cost": 400.00, "list_price": 1200.00, "type": "one-time"},
    {"product_id": "PROD-008", "name": "Premium Support", "category": "Support", "unit_cost": 100.00, "list_price": 299.00, "type": "subscription"},
]

CUSTOMERS = [
    {"customer_id": "CUST-001", "name": "Fjord Shipping AS", "segment": "Enterprise", "region": "Nordics", "country": "NO", "acquisition_date": "2022-06-15"},
    {"customer_id": "CUST-002", "name": "Baltic Timber Group", "segment": "Mid-Market", "region": "Nordics", "country": "FI", "acquisition_date": "2022-09-01"},
    {"customer_id": "CUST-003", "name": "Volvo Logistics AB", "segment": "Enterprise", "region": "Nordics", "country": "SE", "acquisition_date": "2022-11-20"},
    {"customer_id": "CUST-004", "name": "Reykjavik Energy", "segment": "Mid-Market", "region": "Nordics", "country": "IS", "acquisition_date": "2023-01-10"},
    {"customer_id": "CUST-005", "name": "TechHub Berlin GmbH", "segment": "Mid-Market", "region": "DACH", "country": "DE", "acquisition_date": "2023-02-15"},
    {"customer_id": "CUST-006", "name": "Zurich Insurance Tech", "segment": "Enterprise", "region": "DACH", "country": "CH", "acquisition_date": "2023-03-01"},
    {"customer_id": "CUST-007", "name": "Rotterdam Port Auth", "segment": "Enterprise", "region": "Benelux", "country": "NL", "acquisition_date": "2023-04-20"},
    {"customer_id": "CUST-008", "name": "Copenhagen Startups", "segment": "SMB", "region": "Nordics", "country": "DK", "acquisition_date": "2023-05-10"},
    {"customer_id": "CUST-009", "name": "Helsinki Digital Oy", "segment": "SMB", "region": "Nordics", "country": "FI", "acquisition_date": "2023-06-01"},
    {"customer_id": "CUST-010", "name": "Warsaw Fintech Sp.", "segment": "Mid-Market", "region": "CEE", "country": "PL", "acquisition_date": "2023-07-15"},
    {"customer_id": "CUST-011", "name": "Paris Analytics SAS", "segment": "Enterprise", "region": "Western Europe", "country": "FR", "acquisition_date": "2023-08-01"},
    {"customer_id": "CUST-012", "name": "London DataCo Ltd", "segment": "Enterprise", "region": "UK", "country": "GB", "acquisition_date": "2023-09-10"},
    {"customer_id": "CUST-013", "name": "Milan Software Srl", "segment": "Mid-Market", "region": "Southern Europe", "country": "IT", "acquisition_date": "2023-10-20"},
    {"customer_id": "CUST-014", "name": "Madrid Cloud SA", "segment": "SMB", "region": "Southern Europe", "country": "ES", "acquisition_date": "2023-11-05"},
    {"customer_id": "CUST-015", "name": "Vienna Systems AG", "segment": "Mid-Market", "region": "DACH", "country": "AT", "acquisition_date": "2023-12-01"},
    {"customer_id": "CUST-016", "name": "Dublin SaaS Ltd", "segment": "SMB", "region": "UK", "country": "IE", "acquisition_date": "2024-01-15"},
    {"customer_id": "CUST-017", "name": "Tallinn Tech OÜ", "segment": "SMB", "region": "Nordics", "country": "EE", "acquisition_date": "2024-02-20"},
    {"customer_id": "CUST-018", "name": "Prague Analytics sro", "segment": "Mid-Market", "region": "CEE", "country": "CZ", "acquisition_date": "2024-03-10"},
    {"customer_id": "CUST-019", "name": "Brussels EU Services", "segment": "Enterprise", "region": "Benelux", "country": "BE", "acquisition_date": "2024-05-01"},
    {"customer_id": "CUST-020", "name": "Lisbon Waves Lda", "segment": "SMB", "region": "Southern Europe", "country": "PT", "acquisition_date": "2024-06-15"},
    {"customer_id": "CUST-021", "name": "Riga Innovations SIA", "segment": "SMB", "region": "Nordics", "country": "LV", "acquisition_date": "2024-08-01"},
    {"customer_id": "CUST-022", "name": "Munich Engineering", "segment": "Enterprise", "region": "DACH", "country": "DE", "acquisition_date": "2024-09-10"},
    {"customer_id": "CUST-023", "name": "Amsterdam AI BV", "segment": "Mid-Market", "region": "Benelux", "country": "NL", "acquisition_date": "2024-10-20"},
    {"customer_id": "CUST-024", "name": "Stockholm Growth AB", "segment": "Mid-Market", "region": "Nordics", "country": "SE", "acquisition_date": "2024-11-05"},
    {"customer_id": "CUST-025", "name": "Oslo Ventures AS", "segment": "SMB", "region": "Nordics", "country": "NO", "acquisition_date": "2025-01-10"},
]

EMPLOYEES = [
    {"employee_id": "EMP-001", "name": "Mikko Virtanen", "department": "Engineering", "role": "CTO", "hire_date": "2021-01-15", "annual_salary": 120000, "active": True},
    {"employee_id": "EMP-002", "name": "Anna Lindqvist", "department": "Sales", "role": "VP Sales", "hire_date": "2021-03-01", "annual_salary": 105000, "active": True},
    {"employee_id": "EMP-003", "name": "Lars Johansen", "department": "Engineering", "role": "Senior Developer", "hire_date": "2021-06-15", "annual_salary": 85000, "active": True},
    {"employee_id": "EMP-004", "name": "Sanna Korhonen", "department": "Marketing", "role": "Marketing Director", "hire_date": "2021-09-01", "annual_salary": 90000, "active": True},
    {"employee_id": "EMP-005", "name": "Erik Nilsson", "department": "Engineering", "role": "Developer", "hire_date": "2022-01-10", "annual_salary": 72000, "active": True},
    {"employee_id": "EMP-006", "name": "Katja Mäkelä", "department": "Finance", "role": "CFO", "hire_date": "2022-02-01", "annual_salary": 110000, "active": True},
    {"employee_id": "EMP-007", "name": "Olav Bergström", "department": "Sales", "role": "Account Executive", "hire_date": "2022-04-15", "annual_salary": 65000, "active": True},
    {"employee_id": "EMP-008", "name": "Liisa Heikkinen", "department": "Support", "role": "Support Lead", "hire_date": "2022-06-01", "annual_salary": 58000, "active": True},
    {"employee_id": "EMP-009", "name": "Henrik Dahl", "department": "Engineering", "role": "Developer", "hire_date": "2022-08-15", "annual_salary": 70000, "active": True},
    {"employee_id": "EMP-010", "name": "Marja Salminen", "department": "HR", "role": "HR Manager", "hire_date": "2022-10-01", "annual_salary": 68000, "active": True},
    {"employee_id": "EMP-011", "name": "Pekka Laine", "department": "Engineering", "role": "Senior Developer", "hire_date": "2023-01-15", "annual_salary": 82000, "active": True},
    {"employee_id": "EMP-012", "name": "Sofia Andersen", "department": "Sales", "role": "Account Executive", "hire_date": "2023-03-01", "annual_salary": 63000, "active": True},
    {"employee_id": "EMP-013", "name": "Tuomas Rantanen", "department": "Engineering", "role": "DevOps Engineer", "hire_date": "2023-04-15", "annual_salary": 78000, "active": True},
    {"employee_id": "EMP-014", "name": "Elina Koskinen", "department": "Marketing", "role": "Content Specialist", "hire_date": "2023-06-01", "annual_salary": 52000, "active": True},
    {"employee_id": "EMP-015", "name": "Jonas Eriksson", "department": "Support", "role": "Support Engineer", "hire_date": "2023-07-15", "annual_salary": 50000, "active": True},
    {"employee_id": "EMP-016", "name": "Aino Nieminen", "department": "Engineering", "role": "Developer", "hire_date": "2023-09-01", "annual_salary": 68000, "active": True},
    {"employee_id": "EMP-017", "name": "Magnus Olsen", "department": "Sales", "role": "Sales Engineer", "hire_date": "2023-10-15", "annual_salary": 70000, "active": True},
    {"employee_id": "EMP-018", "name": "Riikka Hämäläinen", "department": "Finance", "role": "Accountant", "hire_date": "2023-11-01", "annual_salary": 55000, "active": True},
    {"employee_id": "EMP-019", "name": "Nils Svensson", "department": "Engineering", "role": "Senior Developer", "hire_date": "2024-01-15", "annual_salary": 84000, "active": True},
    {"employee_id": "EMP-020", "name": "Kaisa Lahtinen", "department": "Operations", "role": "Operations Manager", "hire_date": "2024-02-01", "annual_salary": 72000, "active": True},
    {"employee_id": "EMP-021", "name": "Ville Aalto", "department": "Engineering", "role": "Developer", "hire_date": "2024-04-15", "annual_salary": 67000, "active": True},
    {"employee_id": "EMP-022", "name": "Hanna Björk", "department": "Sales", "role": "Account Executive", "hire_date": "2024-06-01", "annual_salary": 62000, "active": True},
    {"employee_id": "EMP-023", "name": "Tero Koponen", "department": "Engineering", "role": "QA Engineer", "hire_date": "2024-07-15", "annual_salary": 60000, "active": True},
    {"employee_id": "EMP-024", "name": "Iida Peltonen", "department": "Marketing", "role": "Growth Marketer", "hire_date": "2024-09-01", "annual_salary": 58000, "active": True},
    {"employee_id": "EMP-025", "name": "Oskar Holm", "department": "Support", "role": "Support Engineer", "hire_date": "2024-10-15", "annual_salary": 48000, "active": True},
    {"employee_id": "EMP-026", "name": "Jens Kristiansen", "department": "Engineering", "role": "Developer", "hire_date": "2025-01-15", "annual_salary": 69000, "active": True},
    {"employee_id": "EMP-027", "name": "Maria Kallio", "department": "Sales", "role": "Account Executive", "hire_date": "2025-03-01", "annual_salary": 61000, "active": True},
    {"employee_id": "EMP-028", "name": "Riku Toivonen", "department": "Engineering", "role": "Developer", "hire_date": "2022-05-01", "annual_salary": 71000, "active": False},  # Left 2024-06
]

VENDORS = [
    {"vendor_id": "VEND-001", "name": "AWS Europe", "category": "Cloud Infrastructure", "country": "IE", "payment_terms": 30},
    {"vendor_id": "VEND-002", "name": "Hetzner Online", "category": "Cloud Infrastructure", "country": "DE", "payment_terms": 14},
    {"vendor_id": "VEND-003", "name": "JetBrains sro", "category": "Software Licenses", "country": "CZ", "payment_terms": 30},
    {"vendor_id": "VEND-004", "name": "Telia Finland Oyj", "category": "Telecommunications", "country": "FI", "payment_terms": 14},
    {"vendor_id": "VEND-005", "name": "Leaseplan Finland", "category": "Office & Facilities", "country": "FI", "payment_terms": 30},
    {"vendor_id": "VEND-006", "name": "Smartly.io", "category": "Marketing Services", "country": "FI", "payment_terms": 30},
    {"vendor_id": "VEND-007", "name": "Accountor Group", "category": "Professional Services", "country": "FI", "payment_terms": 14},
    {"vendor_id": "VEND-008", "name": "Visma Solutions", "category": "Software Licenses", "country": "NO", "payment_terms": 30},
    {"vendor_id": "VEND-009", "name": "Stripe Payments", "category": "Payment Processing", "country": "IE", "payment_terms": 7},
    {"vendor_id": "VEND-010", "name": "DataDog Inc", "category": "Monitoring", "country": "US", "payment_terms": 30},
]

# Chart of accounts
ACCOUNTS = [
    # Assets (1xxx)
    {"account_code": "1000", "name": "Cash and Bank", "type": "Asset", "subtype": "Current Asset", "normal_balance": "debit"},
    {"account_code": "1100", "name": "Accounts Receivable", "type": "Asset", "subtype": "Current Asset", "normal_balance": "debit"},
    {"account_code": "1200", "name": "Prepaid Expenses", "type": "Asset", "subtype": "Current Asset", "normal_balance": "debit"},
    {"account_code": "1500", "name": "Equipment", "type": "Asset", "subtype": "Fixed Asset", "normal_balance": "debit"},
    {"account_code": "1510", "name": "Accumulated Depreciation", "type": "Asset", "subtype": "Fixed Asset", "normal_balance": "credit"},
    # Liabilities (2xxx)
    {"account_code": "2000", "name": "Accounts Payable", "type": "Liability", "subtype": "Current Liability", "normal_balance": "credit"},
    {"account_code": "2100", "name": "Accrued Expenses", "type": "Liability", "subtype": "Current Liability", "normal_balance": "credit"},
    {"account_code": "2200", "name": "Deferred Revenue", "type": "Liability", "subtype": "Current Liability", "normal_balance": "credit"},
    {"account_code": "2300", "name": "Payroll Taxes Payable", "type": "Liability", "subtype": "Current Liability", "normal_balance": "credit"},
    {"account_code": "2500", "name": "Long-term Loan", "type": "Liability", "subtype": "Long-term Liability", "normal_balance": "credit"},
    # Equity (3xxx)
    {"account_code": "3000", "name": "Share Capital", "type": "Equity", "subtype": "Equity", "normal_balance": "credit"},
    {"account_code": "3100", "name": "Retained Earnings", "type": "Equity", "subtype": "Equity", "normal_balance": "credit"},
    # Revenue (4xxx)
    {"account_code": "4000", "name": "Subscription Revenue", "type": "Revenue", "subtype": "Operating Revenue", "normal_balance": "credit"},
    {"account_code": "4100", "name": "Services Revenue", "type": "Revenue", "subtype": "Operating Revenue", "normal_balance": "credit"},
    {"account_code": "4200", "name": "Support Revenue", "type": "Revenue", "subtype": "Operating Revenue", "normal_balance": "credit"},
    # COGS (5xxx)
    {"account_code": "5000", "name": "Cloud Infrastructure", "type": "Expense", "subtype": "Cost of Goods Sold", "normal_balance": "debit"},
    {"account_code": "5100", "name": "Third-party Licenses", "type": "Expense", "subtype": "Cost of Goods Sold", "normal_balance": "debit"},
    # Operating Expenses (6xxx-7xxx)
    {"account_code": "6000", "name": "Salaries & Wages", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6100", "name": "Employee Benefits", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6200", "name": "Payroll Taxes", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6300", "name": "Rent & Facilities", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6400", "name": "Marketing & Advertising", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6500", "name": "Travel & Entertainment", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6600", "name": "Software & Subscriptions", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6700", "name": "Professional Services", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6800", "name": "Depreciation", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "6900", "name": "Telecommunications", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "7000", "name": "Insurance", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    {"account_code": "7100", "name": "Miscellaneous Expenses", "type": "Expense", "subtype": "Operating Expense", "normal_balance": "debit"},
    # Other (8xxx)
    {"account_code": "8000", "name": "Interest Expense", "type": "Expense", "subtype": "Other Expense", "normal_balance": "debit"},
    {"account_code": "8100", "name": "Interest Income", "type": "Revenue", "subtype": "Other Revenue", "normal_balance": "credit"},
]

SALESPERSON_IDS = ["EMP-002", "EMP-007", "EMP-012", "EMP-017", "EMP-022", "EMP-027"]

# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------

def rand_date_in_month(year, month):
    """Random business day in a given month."""
    start = date(year, month, 1)
    if month == 12:
        end = date(year, 12, 31)
    else:
        end = date(year, month + 1, 1) - timedelta(days=1)
    d = start + timedelta(days=random.randint(0, (end - start).days))
    # Push weekends to Friday
    if d.weekday() == 5:
        d -= timedelta(days=1)
    elif d.weekday() == 6:
        d -= timedelta(days=2)
    return d

def month_range(start_year, start_month, end_year, end_month):
    """Yield (year, month) tuples."""
    y, m = start_year, start_month
    while (y, m) <= (end_year, end_month):
        yield y, m
        m += 1
        if m > 12:
            m = 1
            y += 1

def seasonality(month):
    """Seasonal multiplier: Q1 strong start, summer dip, Q4 strong."""
    factors = {1: 1.05, 2: 1.00, 3: 1.10, 4: 1.05, 5: 0.95, 6: 0.85,
               7: 0.75, 8: 0.80, 9: 1.00, 10: 1.05, 11: 1.10, 12: 1.15}
    return factors[month]

def growth_factor(year, month):
    """Year-over-year growth (~30% annually, compounding monthly)."""
    months_from_start = (year - 2023) * 12 + (month - 1)
    return 1.0 * (1.30 ** (months_from_start / 12))

def active_customers_at(year, month):
    """Customers acquired on or before this month."""
    cutoff = f"{year:04d}-{month:02d}-28"
    return [c for c in CUSTOMERS if c["acquisition_date"] <= cutoff]

def active_employees_at(year, month):
    """Employees hired on or before this month who are still active (or left later)."""
    cutoff = f"{year:04d}-{month:02d}-28"
    result = []
    for e in EMPLOYEES:
        if e["hire_date"] <= cutoff:
            # EMP-028 left in 2024-06
            if e["employee_id"] == "EMP-028" and (year, month) >= (2024, 7):
                continue
            result.append(e)
    return result

def round2(x):
    return round(x, 2)

# ---------------------------------------------------------------------------
# Generate sales transactions
# ---------------------------------------------------------------------------

sales = []
sale_id = 1
for year, month in month_range(2023, 1, 2025, 12):
    custs = active_customers_at(year, month)
    if not custs:
        continue

    # Base: each customer makes 1-3 purchases per month, scaled by season/growth
    base_txns = max(3, int(len(custs) * 1.5 * seasonality(month) * growth_factor(year, month) * random.uniform(0.8, 1.2)))

    for _ in range(base_txns):
        cust = random.choice(custs)
        prod = random.choice(PRODUCTS)
        d = rand_date_in_month(year, month)
        qty = 1
        if prod["type"] == "subscription":
            qty = random.choices([1, 2, 3, 5, 10], weights=[40, 25, 15, 10, 10])[0]

        # Some discounts for Enterprise
        discount_pct = 0
        if cust["segment"] == "Enterprise":
            discount_pct = random.choice([0, 5, 10, 15, 20])
        elif cust["segment"] == "Mid-Market":
            discount_pct = random.choice([0, 0, 5, 10])

        unit_price = prod["list_price"]
        discount_amount = round2(unit_price * qty * discount_pct / 100)
        total = round2(unit_price * qty - discount_amount)

        salesperson = random.choice(SALESPERSON_IDS)

        sales.append({
            "sale_id": f"SALE-{sale_id:05d}",
            "date": d.isoformat(),
            "customer_id": cust["customer_id"],
            "customer_name": cust["name"],
            "product_id": prod["product_id"],
            "product_name": prod["name"],
            "product_category": prod["category"],
            "quantity": qty,
            "unit_price": unit_price,
            "discount_percent": discount_pct,
            "discount_amount": discount_amount,
            "total_amount": total,
            "salesperson_id": salesperson,
            "region": cust["region"],
            "country": cust["country"],
            "segment": cust["segment"],
            "payment_type": prod["type"],
        })
        sale_id += 1

sales.sort(key=lambda x: x["date"])
print(f"Sales transactions: {len(sales)}")

# ---------------------------------------------------------------------------
# Generate accounts receivable (from sales)
# ---------------------------------------------------------------------------

ar_records = []
for s in sales:
    invoice_date = date.fromisoformat(s["date"])
    due_date = invoice_date + timedelta(days=random.choice([15, 30, 30, 30, 45]))

    # Determine payment status based on age
    age = (date(2025, 12, 31) - due_date).days
    if age > 90:
        status = "paid"
        paid_date = (due_date + timedelta(days=random.randint(0, 30))).isoformat()
    elif age > 30:
        status = random.choices(["paid", "overdue"], weights=[92, 8])[0]
        paid_date = (due_date + timedelta(days=random.randint(0, 20))).isoformat() if status == "paid" else None
    elif age > 0:
        status = random.choices(["paid", "outstanding", "overdue"], weights=[70, 20, 10])[0]
        paid_date = (due_date + timedelta(days=random.randint(0, 10))).isoformat() if status == "paid" else None
    else:
        status = "outstanding"
        paid_date = None

    ar_records.append({
        "invoice_id": f"INV-{s['sale_id'].split('-')[1]}",
        "sale_id": s["sale_id"],
        "customer_id": s["customer_id"],
        "customer_name": s["customer_name"],
        "invoice_date": s["date"],
        "due_date": due_date.isoformat(),
        "amount": s["total_amount"],
        "status": status,
        "paid_date": paid_date,
        "aging_bucket": "current" if age <= 30 else ("31-60" if age <= 60 else ("61-90" if age <= 90 else "90+")),
    })

# ---------------------------------------------------------------------------
# Aggregate monthly revenue from sales
# ---------------------------------------------------------------------------

monthly_revenue = {}  # (year, month) -> {subscription, services, support}
for s in sales:
    d = date.fromisoformat(s["date"])
    key = (d.year, d.month)
    if key not in monthly_revenue:
        monthly_revenue[key] = {"subscription": 0, "services": 0, "support": 0}

    cat = s["product_category"]
    if cat in ("Analytics", "Integration", "Platform"):
        monthly_revenue[key]["subscription"] += s["total_amount"]
    elif cat == "Services":
        monthly_revenue[key]["services"] += s["total_amount"]
    elif cat == "Support":
        monthly_revenue[key]["support"] += s["total_amount"]

# ---------------------------------------------------------------------------
# Generate payroll records
# ---------------------------------------------------------------------------

payroll = []
for year, month in month_range(2023, 1, 2025, 12):
    emps = active_employees_at(year, month)
    for emp in emps:
        monthly_salary = round2(emp["annual_salary"] / 12)
        tax_rate = 0.20 + random.uniform(-0.02, 0.02)  # ~20% income tax
        employer_contributions = round2(monthly_salary * 0.25)  # Finnish ~25% employer social contributions
        employee_tax = round2(monthly_salary * tax_rate)
        benefits = round2(monthly_salary * 0.05)  # Health, lunch, etc.
        net_pay = round2(monthly_salary - employee_tax - benefits * 0.3)  # Some benefit cost sharing

        payroll.append({
            "payroll_id": f"PAY-{year}{month:02d}-{emp['employee_id'].split('-')[1]}",
            "employee_id": emp["employee_id"],
            "employee_name": emp["name"],
            "department": emp["department"],
            "role": emp["role"],
            "period": f"{year:04d}-{month:02d}",
            "gross_salary": monthly_salary,
            "income_tax": employee_tax,
            "employee_benefits": benefits,
            "net_pay": net_pay,
            "employer_social_contributions": employer_contributions,
            "total_cost_to_company": round2(monthly_salary + employer_contributions + benefits),
        })

print(f"Payroll records: {len(payroll)}")

# ---------------------------------------------------------------------------
# Generate accounts payable
# ---------------------------------------------------------------------------

ap_records = []
ap_id = 1
for year, month in month_range(2023, 1, 2025, 12):
    gf = growth_factor(year, month)

    # AWS - scales with revenue
    rev = monthly_revenue.get((year, month), {"subscription": 0})
    aws_cost = round2(rev["subscription"] * 0.12 * random.uniform(0.9, 1.1))  # ~12% of subscription rev
    if aws_cost > 0:
        d = rand_date_in_month(year, month)
        ap_records.append({
            "ap_id": f"AP-{ap_id:05d}", "vendor_id": "VEND-001", "vendor_name": "AWS Europe",
            "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=30)).isoformat(),
            "amount": aws_cost, "category": "Cloud Infrastructure",
            "status": "paid" if (year, month) < (2025, 11) else "outstanding",
            "description": f"AWS services {year}-{month:02d}",
        })
        ap_id += 1

    # Hetzner - smaller, fixed-ish
    hetzner = round2(random.uniform(800, 1500) * gf)
    d = rand_date_in_month(year, month)
    ap_records.append({
        "ap_id": f"AP-{ap_id:05d}", "vendor_id": "VEND-002", "vendor_name": "Hetzner Online",
        "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=14)).isoformat(),
        "amount": hetzner, "category": "Cloud Infrastructure",
        "status": "paid" if (year, month) < (2025, 12) else "outstanding",
        "description": f"Dedicated servers {year}-{month:02d}",
    })
    ap_id += 1

    # Quarterly vendors
    if month % 3 == 0:
        # JetBrains
        d = rand_date_in_month(year, month)
        num_devs = len([e for e in active_employees_at(year, month) if e["department"] == "Engineering"])
        ap_records.append({
            "ap_id": f"AP-{ap_id:05d}", "vendor_id": "VEND-003", "vendor_name": "JetBrains sro",
            "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=30)).isoformat(),
            "amount": round2(num_devs * 250), "category": "Software Licenses",
            "status": "paid" if (year, month) < (2025, 10) else "outstanding",
            "description": f"IDE licenses Q{month//3} {year}",
        })
        ap_id += 1

    # Monthly recurring
    for vendor_id, vendor_name, category, base_amount, desc in [
        ("VEND-004", "Telia Finland Oyj", "Telecommunications", 450, "Phone & internet"),
        ("VEND-005", "Leaseplan Finland", "Office & Facilities", 3500, "Office lease"),
        ("VEND-008", "Visma Solutions", "Software Licenses", 800, "ERP subscription"),
        ("VEND-010", "DataDog Inc", "Monitoring", 600, "Monitoring subscription"),
    ]:
        d = rand_date_in_month(year, month)
        amount = round2(base_amount * gf * random.uniform(0.95, 1.05))
        terms = next(v["payment_terms"] for v in VENDORS if v["vendor_id"] == vendor_id)
        ap_records.append({
            "ap_id": f"AP-{ap_id:05d}", "vendor_id": vendor_id, "vendor_name": vendor_name,
            "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=terms)).isoformat(),
            "amount": amount, "category": category,
            "status": "paid" if (year, month) < (2025, 11) else random.choice(["paid", "outstanding"]),
            "description": f"{desc} {year}-{month:02d}",
        })
        ap_id += 1

    # Occasional vendors
    if random.random() < 0.3:
        d = rand_date_in_month(year, month)
        ap_records.append({
            "ap_id": f"AP-{ap_id:05d}", "vendor_id": "VEND-006", "vendor_name": "Smartly.io",
            "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=30)).isoformat(),
            "amount": round2(random.uniform(2000, 8000) * gf), "category": "Marketing Services",
            "status": "paid" if (year, month) < (2025, 10) else "outstanding",
            "description": f"Ad campaign services {year}-{month:02d}",
        })
        ap_id += 1

    if random.random() < 0.15:
        d = rand_date_in_month(year, month)
        ap_records.append({
            "ap_id": f"AP-{ap_id:05d}", "vendor_id": "VEND-007", "vendor_name": "Accountor Group",
            "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=14)).isoformat(),
            "amount": round2(random.uniform(1500, 5000)), "category": "Professional Services",
            "status": "paid",
            "description": f"Consulting services {year}-{month:02d}",
        })
        ap_id += 1

    # Stripe - percentage of revenue
    total_rev = sum(monthly_revenue.get((year, month), {"subscription": 0, "services": 0, "support": 0}).values())
    if total_rev > 0:
        d = date(year, month, 28) if month != 2 else date(year, month, 27)
        stripe_fee = round2(total_rev * 0.029 + len(sales) / 36 * 0.25)  # 2.9% + per-txn
        ap_records.append({
            "ap_id": f"AP-{ap_id:05d}", "vendor_id": "VEND-009", "vendor_name": "Stripe Payments",
            "invoice_date": d.isoformat(), "due_date": (d + timedelta(days=7)).isoformat(),
            "amount": round2(stripe_fee), "category": "Payment Processing",
            "status": "paid" if (year, month) < (2025, 12) else "outstanding",
            "description": f"Payment processing fees {year}-{month:02d}",
        })
        ap_id += 1

ap_records.sort(key=lambda x: x["invoice_date"])
print(f"AP records: {len(ap_records)}")

# ---------------------------------------------------------------------------
# Generate general ledger (journal entries)
# ---------------------------------------------------------------------------

journal = []
je_id = 1

for year, month in month_range(2023, 1, 2025, 12):
    d_mid = date(year, month, 15)
    d_end = date(year, month, 28) if month != 2 else date(year, month, 27)

    rev = monthly_revenue.get((year, month), {"subscription": 0, "services": 0, "support": 0})

    # Revenue recognition
    if rev["subscription"] > 0:
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1100", "account_name": "Accounts Receivable", "description": f"Subscription revenue {year}-{month:02d}", "debit": round2(rev["subscription"]), "credit": 0, "department": "Sales", "source": "revenue"})
        je_id += 1
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "4000", "account_name": "Subscription Revenue", "description": f"Subscription revenue {year}-{month:02d}", "debit": 0, "credit": round2(rev["subscription"]), "department": "Sales", "source": "revenue"})
        je_id += 1

    if rev["services"] > 0:
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1100", "account_name": "Accounts Receivable", "description": f"Services revenue {year}-{month:02d}", "debit": round2(rev["services"]), "credit": 0, "department": "Sales", "source": "revenue"})
        je_id += 1
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "4100", "account_name": "Services Revenue", "description": f"Services revenue {year}-{month:02d}", "debit": 0, "credit": round2(rev["services"]), "department": "Sales", "source": "revenue"})
        je_id += 1

    if rev["support"] > 0:
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1100", "account_name": "Accounts Receivable", "description": f"Support revenue {year}-{month:02d}", "debit": round2(rev["support"]), "credit": 0, "department": "Sales", "source": "revenue"})
        je_id += 1
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "4200", "account_name": "Support Revenue", "description": f"Support revenue {year}-{month:02d}", "debit": 0, "credit": round2(rev["support"]), "department": "Sales", "source": "revenue"})
        je_id += 1

    # Cash collection (from AR)
    total_rev_month = round2(rev["subscription"] + rev["services"] + rev["support"])
    collected = round2(total_rev_month * random.uniform(0.85, 0.95))
    if collected > 0:
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1000", "account_name": "Cash and Bank", "description": f"Customer collections {year}-{month:02d}", "debit": collected, "credit": 0, "department": "Finance", "source": "collection"})
        je_id += 1
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1100", "account_name": "Accounts Receivable", "description": f"Customer collections {year}-{month:02d}", "debit": 0, "credit": collected, "department": "Finance", "source": "collection"})
        je_id += 1

    # Payroll entries
    month_payroll = [p for p in payroll if p["period"] == f"{year:04d}-{month:02d}"]
    total_gross = round2(sum(p["gross_salary"] for p in month_payroll))
    total_employer = round2(sum(p["employer_social_contributions"] for p in month_payroll))
    total_benefits = round2(sum(p["employee_benefits"] for p in month_payroll))
    total_tax = round2(sum(p["income_tax"] for p in month_payroll))
    total_net = round2(sum(p["net_pay"] for p in month_payroll))

    if total_gross > 0:
        # Salary expense
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "6000", "account_name": "Salaries & Wages", "description": f"Payroll {year}-{month:02d}", "debit": total_gross, "credit": 0, "department": "HR", "source": "payroll"})
        je_id += 1
        # Employer contributions
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "6200", "account_name": "Payroll Taxes", "description": f"Employer contributions {year}-{month:02d}", "debit": total_employer, "credit": 0, "department": "HR", "source": "payroll"})
        je_id += 1
        # Benefits
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "6100", "account_name": "Employee Benefits", "description": f"Employee benefits {year}-{month:02d}", "debit": total_benefits, "credit": 0, "department": "HR", "source": "payroll"})
        je_id += 1
        # Cash payment
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1000", "account_name": "Cash and Bank", "description": f"Net payroll paid {year}-{month:02d}", "debit": 0, "credit": total_net, "department": "HR", "source": "payroll"})
        je_id += 1
        # Tax liability
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "2300", "account_name": "Payroll Taxes Payable", "description": f"Payroll taxes withheld {year}-{month:02d}", "debit": 0, "credit": round2(total_tax + total_employer), "department": "HR", "source": "payroll"})
        je_id += 1
        # Benefits accrual offset
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "2100", "account_name": "Accrued Expenses", "description": f"Benefits accrual {year}-{month:02d}", "debit": 0, "credit": round2(total_gross - total_net - total_tax + total_benefits), "department": "HR", "source": "payroll"})
        je_id += 1

    # AP payments (vendor expenses)
    month_ap = [a for a in ap_records if a["invoice_date"][:7] == f"{year:04d}-{month:02d}"]
    for ap in month_ap:
        # Expense the cost
        acct_code = "5000"  # default COGS
        acct_name = "Cloud Infrastructure"
        dept = "Operations"
        if ap["category"] == "Cloud Infrastructure":
            acct_code, acct_name = "5000", "Cloud Infrastructure"
        elif ap["category"] == "Software Licenses":
            acct_code, acct_name = "5100", "Third-party Licenses"
        elif ap["category"] == "Telecommunications":
            acct_code, acct_name, dept = "6900", "Telecommunications", "Operations"
        elif ap["category"] == "Office & Facilities":
            acct_code, acct_name, dept = "6300", "Rent & Facilities", "Operations"
        elif ap["category"] == "Marketing Services":
            acct_code, acct_name, dept = "6400", "Marketing & Advertising", "Marketing"
        elif ap["category"] == "Professional Services":
            acct_code, acct_name, dept = "6700", "Professional Services", "Finance"
        elif ap["category"] == "Payment Processing":
            acct_code, acct_name, dept = "7100", "Miscellaneous Expenses", "Finance"
        elif ap["category"] == "Monitoring":
            acct_code, acct_name, dept = "6600", "Software & Subscriptions", "Engineering"

        journal.append({"journal_id": f"JE-{je_id:05d}", "date": ap["invoice_date"], "account_code": acct_code, "account_name": acct_name, "description": ap["description"], "debit": ap["amount"], "credit": 0, "department": dept, "source": "ap"})
        je_id += 1
        journal.append({"journal_id": f"JE-{je_id:05d}", "date": ap["invoice_date"], "account_code": "2000", "account_name": "Accounts Payable", "description": ap["description"], "debit": 0, "credit": ap["amount"], "department": dept, "source": "ap"})
        je_id += 1

        # Pay the AP (if paid)
        if ap["status"] == "paid":
            pay_date = (date.fromisoformat(ap["due_date"]) + timedelta(days=random.randint(-5, 10)))
            journal.append({"journal_id": f"JE-{je_id:05d}", "date": pay_date.isoformat(), "account_code": "2000", "account_name": "Accounts Payable", "description": f"Payment: {ap['description']}", "debit": ap["amount"], "credit": 0, "department": "Finance", "source": "payment"})
            je_id += 1
            journal.append({"journal_id": f"JE-{je_id:05d}", "date": pay_date.isoformat(), "account_code": "1000", "account_name": "Cash and Bank", "description": f"Payment: {ap['description']}", "debit": 0, "credit": ap["amount"], "department": "Finance", "source": "payment"})
            je_id += 1

    # Monthly depreciation
    depreciation = round2(1200 * gf)
    journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "6800", "account_name": "Depreciation", "description": f"Monthly depreciation {year}-{month:02d}", "debit": depreciation, "credit": 0, "department": "Finance", "source": "depreciation"})
    je_id += 1
    journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1510", "account_name": "Accumulated Depreciation", "description": f"Monthly depreciation {year}-{month:02d}", "debit": 0, "credit": depreciation, "department": "Finance", "source": "depreciation"})
    je_id += 1

    # Interest on loan (200k loan taken 2022)
    interest = round2(200000 * 0.045 / 12)  # 4.5% annual
    journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "8000", "account_name": "Interest Expense", "description": f"Loan interest {year}-{month:02d}", "debit": interest, "credit": 0, "department": "Finance", "source": "interest"})
    je_id += 1
    journal.append({"journal_id": f"JE-{je_id:05d}", "date": d_end.isoformat(), "account_code": "1000", "account_name": "Cash and Bank", "description": f"Loan interest payment {year}-{month:02d}", "debit": 0, "credit": interest, "department": "Finance", "source": "interest"})
    je_id += 1

journal.sort(key=lambda x: x["date"])
print(f"Journal entries: {len(journal)}")

# ---------------------------------------------------------------------------
# Generate income statement (monthly P&L)
# ---------------------------------------------------------------------------

income_statement = []
for year, month in month_range(2023, 1, 2025, 12):
    period = f"{year:04d}-{month:02d}"
    quarter = f"Q{(month - 1) // 3 + 1}"

    rev = monthly_revenue.get((year, month), {"subscription": 0, "services": 0, "support": 0})
    total_rev_m = round2(rev["subscription"] + rev["services"] + rev["support"])

    # COGS from AP
    month_ap = [a for a in ap_records if a["invoice_date"][:7] == period]
    cogs = round2(sum(a["amount"] for a in month_ap if a["category"] in ("Cloud Infrastructure", "Monitoring")))

    gross_profit = round2(total_rev_m - cogs)

    # OpEx from payroll
    month_pay = [p for p in payroll if p["period"] == period]
    salaries = round2(sum(p["gross_salary"] for p in month_pay))
    benefits = round2(sum(p["employee_benefits"] for p in month_pay))
    payroll_taxes = round2(sum(p["employer_social_contributions"] for p in month_pay))

    # Other OpEx from AP
    rent = round2(sum(a["amount"] for a in month_ap if a["category"] == "Office & Facilities"))
    marketing = round2(sum(a["amount"] for a in month_ap if a["category"] == "Marketing Services"))
    software = round2(sum(a["amount"] for a in month_ap if a["category"] == "Software Licenses"))
    telecom = round2(sum(a["amount"] for a in month_ap if a["category"] == "Telecommunications"))
    professional = round2(sum(a["amount"] for a in month_ap if a["category"] == "Professional Services"))
    payment_fees = round2(sum(a["amount"] for a in month_ap if a["category"] == "Payment Processing"))

    gf_m = growth_factor(year, month)
    depreciation = round2(1200 * gf_m)
    travel = round2(random.uniform(500, 3000) * gf_m * seasonality(month))
    insurance = round2(800 * gf_m)
    misc = round2(random.uniform(200, 800) * gf_m)

    total_opex = round2(salaries + benefits + payroll_taxes + rent + marketing + software + telecom + professional + payment_fees + depreciation + travel + insurance + misc)
    operating_income = round2(gross_profit - total_opex)

    interest_expense = round2(200000 * 0.045 / 12)
    interest_income = round2(random.uniform(50, 300) * gf_m)

    net_income = round2(operating_income - interest_expense + interest_income)

    income_statement.append({
        "period": period,
        "year": year,
        "quarter": quarter,
        "month": month,
        "subscription_revenue": rev["subscription"],
        "services_revenue": rev["services"],
        "support_revenue": rev["support"],
        "total_revenue": total_rev_m,
        "cost_of_goods_sold": cogs,
        "gross_profit": gross_profit,
        "gross_margin_pct": round2(gross_profit / total_rev_m * 100) if total_rev_m > 0 else 0,
        "salaries": salaries,
        "employee_benefits": benefits,
        "payroll_taxes": payroll_taxes,
        "rent_facilities": rent,
        "marketing_advertising": marketing,
        "software_subscriptions": software,
        "telecommunications": telecom,
        "professional_services": professional,
        "payment_processing": payment_fees,
        "depreciation": depreciation,
        "travel_entertainment": travel,
        "insurance": insurance,
        "miscellaneous": misc,
        "total_operating_expenses": total_opex,
        "operating_income": operating_income,
        "interest_expense": interest_expense,
        "interest_income": interest_income,
        "net_income": net_income,
        "net_margin_pct": round2(net_income / total_rev_m * 100) if total_rev_m > 0 else 0,
    })

# ---------------------------------------------------------------------------
# Generate projects/tasks (for kanban view)
# ---------------------------------------------------------------------------

PROJECT_STATUSES = ["backlog", "planned", "in-progress", "review", "done", "cancelled"]
PROJECTS = [
    {"project_id": "PROJ-001", "name": "Platform v2.0 Migration", "department": "Engineering", "start_date": "2024-01-15", "target_date": "2024-06-30", "status": "done", "budget": 150000, "actual_spend": 142500, "priority": "high", "owner_id": "EMP-001"},
    {"project_id": "PROJ-002", "name": "GDPR Compliance Audit", "department": "Finance", "start_date": "2024-02-01", "target_date": "2024-04-30", "status": "done", "budget": 35000, "actual_spend": 38200, "priority": "high", "owner_id": "EMP-006"},
    {"project_id": "PROJ-003", "name": "Nordic Expansion Campaign", "department": "Marketing", "start_date": "2024-03-01", "target_date": "2024-09-30", "status": "done", "budget": 80000, "actual_spend": 72000, "priority": "normal", "owner_id": "EMP-004"},
    {"project_id": "PROJ-004", "name": "Customer Portal Redesign", "department": "Engineering", "start_date": "2024-06-01", "target_date": "2024-12-31", "status": "done", "budget": 95000, "actual_spend": 101000, "priority": "normal", "owner_id": "EMP-003"},
    {"project_id": "PROJ-005", "name": "SOC 2 Type II Certification", "department": "Operations", "start_date": "2024-09-01", "target_date": "2025-03-31", "status": "done", "budget": 60000, "actual_spend": 55000, "priority": "high", "owner_id": "EMP-020"},
    {"project_id": "PROJ-006", "name": "AI Analytics Module", "department": "Engineering", "start_date": "2025-01-15", "target_date": "2025-07-31", "status": "in-progress", "budget": 200000, "actual_spend": 98000, "priority": "high", "owner_id": "EMP-001"},
    {"project_id": "PROJ-007", "name": "DACH Market Entry", "department": "Sales", "start_date": "2025-02-01", "target_date": "2025-08-31", "status": "in-progress", "budget": 120000, "actual_spend": 45000, "priority": "high", "owner_id": "EMP-002"},
    {"project_id": "PROJ-008", "name": "Developer API v3", "department": "Engineering", "start_date": "2025-03-01", "target_date": "2025-09-30", "status": "in-progress", "budget": 85000, "actual_spend": 22000, "priority": "normal", "owner_id": "EMP-011"},
    {"project_id": "PROJ-009", "name": "Employee Wellness Program", "department": "HR", "start_date": "2025-04-01", "target_date": "2025-12-31", "status": "planned", "budget": 25000, "actual_spend": 0, "priority": "low", "owner_id": "EMP-010"},
    {"project_id": "PROJ-010", "name": "Data Pipeline Optimization", "department": "Engineering", "start_date": "2025-05-01", "target_date": "2025-10-31", "status": "planned", "budget": 70000, "actual_spend": 0, "priority": "normal", "owner_id": "EMP-013"},
    {"project_id": "PROJ-011", "name": "Partner Integration Program", "department": "Sales", "start_date": "2025-06-01", "target_date": "2025-12-31", "status": "backlog", "budget": 90000, "actual_spend": 0, "priority": "normal", "owner_id": "EMP-002"},
    {"project_id": "PROJ-012", "name": "Mobile App MVP", "department": "Engineering", "start_date": "2025-07-01", "target_date": "2026-03-31", "status": "backlog", "budget": 180000, "actual_spend": 0, "priority": "low", "owner_id": "EMP-001"},
]

# Tasks within projects (for kanban)
TASKS = []
task_id = 1
task_templates = [
    # PROJ-006 AI Analytics Module tasks
    ("PROJ-006", "Research ML frameworks", "done", "EMP-019", "high", "2025-01-20", "2025-02-15"),
    ("PROJ-006", "Design data pipeline architecture", "done", "EMP-013", "high", "2025-02-01", "2025-03-01"),
    ("PROJ-006", "Implement feature extraction", "done", "EMP-005", "normal", "2025-03-01", "2025-04-15"),
    ("PROJ-006", "Build model training pipeline", "in-progress", "EMP-019", "high", "2025-04-01", "2025-05-30"),
    ("PROJ-006", "Create prediction API endpoints", "in-progress", "EMP-016", "normal", "2025-04-15", "2025-06-15"),
    ("PROJ-006", "Dashboard integration", "planned", "EMP-021", "normal", "2025-06-01", "2025-07-15"),
    ("PROJ-006", "Performance testing", "backlog", "EMP-023", "normal", "2025-07-01", "2025-07-31"),
    ("PROJ-006", "Documentation & training", "backlog", "EMP-014", "low", "2025-07-15", "2025-07-31"),
    # PROJ-007 DACH Market Entry tasks
    ("PROJ-007", "Market research & analysis", "done", "EMP-012", "high", "2025-02-01", "2025-03-15"),
    ("PROJ-007", "Localize product for DE/AT/CH", "in-progress", "EMP-009", "high", "2025-03-01", "2025-05-31"),
    ("PROJ-007", "Build partner network", "in-progress", "EMP-017", "normal", "2025-04-01", "2025-07-31"),
    ("PROJ-007", "Hire DACH sales rep", "review", "EMP-010", "high", "2025-03-15", "2025-05-31"),
    ("PROJ-007", "Launch marketing campaign", "planned", "EMP-004", "normal", "2025-06-01", "2025-08-31"),
    ("PROJ-007", "Set up local support", "backlog", "EMP-008", "low", "2025-07-01", "2025-08-31"),
    # PROJ-008 Developer API v3 tasks
    ("PROJ-008", "API design & spec review", "done", "EMP-011", "high", "2025-03-01", "2025-04-01"),
    ("PROJ-008", "Authentication refactor", "in-progress", "EMP-003", "high", "2025-04-01", "2025-05-31"),
    ("PROJ-008", "Implement new endpoints", "in-progress", "EMP-026", "normal", "2025-04-15", "2025-07-15"),
    ("PROJ-008", "Rate limiting & throttling", "planned", "EMP-016", "normal", "2025-06-01", "2025-07-31"),
    ("PROJ-008", "SDK generation (Python, JS, Go)", "planned", "EMP-021", "normal", "2025-07-01", "2025-09-01"),
    ("PROJ-008", "API documentation portal", "backlog", "EMP-014", "low", "2025-08-01", "2025-09-30"),
    # Additional standalone tasks
    ("PROJ-004", "Fix SSO login redirect bug", "done", "EMP-009", "high", "2024-10-01", "2024-10-15"),
    ("PROJ-004", "Responsive dashboard layout", "done", "EMP-005", "normal", "2024-09-01", "2024-11-30"),
    ("PROJ-001", "Database schema migration", "done", "EMP-003", "high", "2024-02-01", "2024-03-15"),
    ("PROJ-001", "Legacy API deprecation", "done", "EMP-011", "normal", "2024-04-01", "2024-06-30"),
    ("PROJ-005", "Security audit remediation", "done", "EMP-013", "high", "2024-11-01", "2025-01-31"),
    ("PROJ-010", "Benchmark current pipelines", "planned", "EMP-013", "high", "2025-05-01", "2025-06-15"),
    ("PROJ-010", "Implement streaming processing", "backlog", "EMP-019", "normal", "2025-06-15", "2025-09-30"),
]

for proj_id, title, status, assignee, priority, start, target in task_templates:
    proj = next(p for p in PROJECTS if p["project_id"] == proj_id)
    emp = next(e for e in EMPLOYEES if e["employee_id"] == assignee)

    estimated_hours = random.choice([20, 40, 60, 80, 120, 160])
    actual_hours = round2(estimated_hours * random.uniform(0.6, 1.4)) if status in ("done", "in-progress", "review") else 0
    if status in ("backlog", "planned"):
        actual_hours = 0
    elif status == "in-progress":
        actual_hours = round2(estimated_hours * random.uniform(0.3, 0.7))

    TASKS.append({
        "task_id": f"TASK-{task_id:04d}",
        "project_id": proj_id,
        "project_name": proj["name"],
        "title": title,
        "status": status,
        "priority": priority,
        "assignee_id": assignee,
        "assignee_name": emp["name"],
        "department": proj["department"],
        "start_date": start,
        "target_date": target,
        "estimated_hours": estimated_hours,
        "actual_hours": actual_hours,
        "tags": proj["department"].lower(),
    })
    task_id += 1

# ---------------------------------------------------------------------------
# Generate inventory
# ---------------------------------------------------------------------------

# For SaaS, inventory is license/seat counts
inventory = []
for year, month in month_range(2023, 1, 2025, 12):
    for prod in PRODUCTS:
        if prod["type"] != "subscription":
            continue
        # Count active subscriptions from sales (last 6 months window)
        if month > 6:
            window_start = f"{year:04d}-{month-6:02d}-01"
        else:
            window_start = f"{year-1:04d}-{month+6:02d}-01"
        active_subs = sum(
            s["quantity"] for s in sales
            if s["product_id"] == prod["product_id"]
            and s["date"] <= f"{year:04d}-{month:02d}-28"
            and s["date"] >= window_start
        )

        inventory.append({
            "period": f"{year:04d}-{month:02d}",
            "product_id": prod["product_id"],
            "product_name": prod["name"],
            "category": prod["category"],
            "active_subscriptions": active_subs,
            "monthly_recurring_revenue": round2(active_subs * prod["list_price"]),
            "unit_cost": prod["unit_cost"],
            "list_price": prod["list_price"],
        })

# ---------------------------------------------------------------------------
# Generate balance sheet (quarterly snapshots)
# ---------------------------------------------------------------------------

balance_sheet = []
# Starting balances (beginning of 2023)
cash = 350000
ar = 45000
prepaid = 12000
equipment = 85000
accum_depr = 15000
ap = 22000
accrued = 8000
deferred_rev = 30000
payroll_tax_payable = 5000
loan = 200000
share_capital = 150000
retained = 62000

for year, month in month_range(2023, 3, 2025, 12):
    if month not in (3, 6, 9, 12):
        continue

    quarter = f"Q{month // 3}"

    # Accumulate 3 months
    for m_offset in range(3):
        m = month - 2 + m_offset
        rev = monthly_revenue.get((year, m), {"subscription": 0, "services": 0, "support": 0})
        total_rev_q = rev["subscription"] + rev["services"] + rev["support"]

        month_pay = [p for p in payroll if p["period"] == f"{year:04d}-{m:02d}"]
        total_payroll_cost = sum(p["total_cost_to_company"] for p in month_pay)

        month_ap_items = [a for a in ap_records if a["invoice_date"][:7] == f"{year:04d}-{m:02d}"]
        total_ap_new = sum(a["amount"] for a in month_ap_items)
        total_ap_paid = sum(a["amount"] for a in month_ap_items if a["status"] == "paid")

        gf_m = growth_factor(year, m)

        collected = total_rev_q * random.uniform(0.85, 0.95)
        cash += collected - total_payroll_cost * 0.8 - total_ap_paid - 200000 * 0.045 / 12
        ar += total_rev_q - collected
        equipment += random.uniform(0, 2000) * gf_m
        accum_depr += 1200 * gf_m
        ap += total_ap_new - total_ap_paid
        deferred_rev = total_rev_q * random.uniform(0.05, 0.15)
        retained += total_rev_q - total_payroll_cost - total_ap_new - 1200 * gf_m - 200000 * 0.045 / 12

    total_assets = round2(cash + ar + prepaid + equipment - accum_depr)
    total_liabilities = round2(ap + accrued + deferred_rev + payroll_tax_payable + loan)
    total_equity = round2(share_capital + retained)

    balance_sheet.append({
        "period": f"{year:04d}-{quarter}",
        "year": year,
        "quarter": quarter,
        "cash_and_bank": round2(cash),
        "accounts_receivable": round2(ar),
        "prepaid_expenses": round2(prepaid),
        "total_current_assets": round2(cash + ar + prepaid),
        "equipment": round2(equipment),
        "accumulated_depreciation": round2(accum_depr),
        "net_fixed_assets": round2(equipment - accum_depr),
        "total_assets": total_assets,
        "accounts_payable": round2(ap),
        "accrued_expenses": round2(accrued),
        "deferred_revenue": round2(deferred_rev),
        "payroll_taxes_payable": round2(payroll_tax_payable),
        "total_current_liabilities": round2(ap + accrued + deferred_rev + payroll_tax_payable),
        "long_term_loan": round2(loan),
        "total_liabilities": total_liabilities,
        "share_capital": round2(share_capital),
        "retained_earnings": round2(retained),
        "total_equity": total_equity,
        "total_liabilities_and_equity": round2(total_liabilities + total_equity),
    })

# ---------------------------------------------------------------------------
# Write all files
# ---------------------------------------------------------------------------

def write_json(filename, data):
    path = OUT / filename
    with open(path, "w") as f:
        json.dump(data, f, indent=2, default=str)
    print(f"  {filename}: {len(data)} records")

print("\nWriting files:")
write_json("chart_of_accounts.json", ACCOUNTS)
write_json("products.json", PRODUCTS)
write_json("customers.json", CUSTOMERS)
write_json("employees.json", EMPLOYEES)
write_json("vendors.json", VENDORS)
write_json("sales_transactions.json", sales)
write_json("accounts_receivable.json", ar_records)
write_json("accounts_payable.json", ap_records)
write_json("general_ledger.json", journal)
write_json("payroll.json", payroll)
write_json("income_statement.json", income_statement)
write_json("balance_sheet.json", balance_sheet)
write_json("projects.json", PROJECTS)
write_json("tasks.json", TASKS)
write_json("inventory.json", inventory)

# Summary
print(f"\nTotal records across all files: {sum(len(x) for x in [ACCOUNTS, PRODUCTS, CUSTOMERS, EMPLOYEES, VENDORS, sales, ar_records, ap_records, journal, payroll, income_statement, balance_sheet, PROJECTS, TASKS, inventory])}")
print(f"Sales transactions: {len(sales)}")
print(f"Journal entries: {len(journal)}")

# Verify journal balance
total_debits = sum(j["debit"] for j in journal)
total_credits = sum(j["credit"] for j in journal)
print(f"\nJournal verification:")
print(f"  Total debits:  {total_debits:,.2f}")
print(f"  Total credits: {total_credits:,.2f}")
print(f"  Difference:    {abs(total_debits - total_credits):,.2f}")
