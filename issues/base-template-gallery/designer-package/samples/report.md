# Workspace consolidation review

**Prepared for the operations group · 22 September 2026 · Analysis window: April to August 2026**

## Executive summary

The team can reduce routine tool switching without forcing every workflow into one system. Consolidating the three most common request paths would remove an estimated 11 hours of weekly coordination. Specialist research and finance archives should remain where they are until retention requirements are confirmed.

## Finding 1: most movement comes from three paths

Staff recorded 486 cross-tool handoffs during the study period. Project intake, approval, and delivery status accounted for 71% of them.

<div class="gp-chart" id="handoffs" aria-label="Cross-tool handoffs by workflow chart"></div>
<script>
gp.chart("#handoffs", {
  width: "container",
  height: 260,
  data: { values: [
    { workflow: "Project intake", handoffs: 142 },
    { workflow: "Approval", handoffs: 111 },
    { workflow: "Delivery status", handoffs: 92 },
    { workflow: "Research", handoffs: 78 },
    { workflow: "Finance archive", handoffs: 63 }
  ]},
  mark: "bar",
  encoding: {
    y: { field: "workflow", type: "nominal", sort: "-x", title: null },
    x: { field: "handoffs", type: "quantitative", title: "Recorded handoffs" }
  }
});
</script>

## Finding 2: the cost is uneven

| Workflow | Weekly volume | Minutes per handoff | Estimated hours/week | Confidence |
|---|---:|---:|---:|---|
| Project intake | 31 | 6 | 3.1 | High |
| Approval | 24 | 9 | 3.6 | Medium |
| Delivery status | 20 | 13 | 4.3 | High |
| Research | 17 | 4 | 1.1 | Low |
| Finance archive | 14 | 8 | 1.9 | Medium |

> The estimate measures coordination time, not the quality or suitability of each specialist system.

## Recommendation

Run a six-week consolidation pilot for project intake, approval, and delivery status:

1. Keep one canonical request record.
2. Link specialist work rather than copying its full contents.
3. Publish a weekly exception list instead of a duplicate status report.
4. Measure completion time and rework against the April to August baseline.

## Limitations

The sample covers 19 staff members and excludes two seasonal workflows. Self-recorded handoff time is rounded to the nearest minute. The analysis therefore supports a bounded pilot, not an organization-wide migration.

## Sources and method

- Anonymized workflow diary, April to August 2026
- Request-system event export, retrieved 15 September 2026
- Six structured staff interviews
- Analysis notebook revision `7c42a1e`

The [review worksheet](../assets/report-review-worksheet.svg) shows the interview prompts used for the decision mapping exercise.
