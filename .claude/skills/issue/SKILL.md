---
name: issue
description: Create a new issue in issues/. Supports epics, tasks, and bugs.
---

# Create Issue

Create a new issue in `issues/open/`.

## Arguments

Argument: $ARGUMENTS

## Process

### 1. Determine Issue Type

From the arguments and context, determine the type:

- **epic** — high-level deliverable grouping multiple tasks
- **task** — concrete actionable work item
- **bug** — something is broken (default if unclear)

### 2. Gather Information

If arguments already provide enough context, use it. Otherwise ask the user interactively for missing details. Only ask what's relevant for the issue type.

**All types:**

- **Title** — short, descriptive title
- **Description** — what and why
- **Reporter** — detect via `whoami`, map to team member
- **Assignee** — ask if not obvious
- **Priority** — normal or high (default: normal)
- **Parent** — parent epic number (for tasks and bugs, if applicable)

**Epic additionally:**

- **Sub-issues** — planned child tasks (can be empty initially)
- **Acceptance criteria** — what "done" looks like

**Task additionally:**

- **Acceptance criteria** — what "done" looks like

**Bug additionally:**

- **Where does it happen?** — which service/page/feature
- **How to reproduce?** — steps or "always happens"
- **Quick test** — command/URL to verify (optional)
- **Screenshots** — file paths to include (optional)

Be smart: if arguments already contain enough detail, don't re-ask. Only ask for what's missing.

### 3. Determine Issue Number

Scan BOTH `issues/open/` and `issues/closed/` directories. Find the highest existing number (from directory names like `07-list-section`), increment by 1. Use zero-padded two-digit format (e.g. `11`).

**Important**: Numbers must be unique and sequential. Never reuse or duplicate a number.

### 4. Generate Directory Name

Create a short kebab-case slug from the issue title. Finnish is fine. Follow existing pattern:

- `01-glasspad-mvp`
- `02-list-section-rendering`

### 5. Create Issue Directory and Files

Create:

```
issues/open/NN-short-slug/
├── item.md
└── (optional: plan.md, screenshots, etc.)
```

### 6. item.md Format

Use the template matching the issue type from `issues/AGENTS.md`.

### 7. Copy Screenshots (bugs only)

If the user provides file paths to screenshots or images, convert the images to avif format and copy them into the issue directory and reference them in item.md with relative paths.

### 8. Confirm

Show the created issue path and a brief summary of what was filed.

## Notes

- Use today's date for the `created` and `updated` fields
- Write issue content in English
- Keep the slug reasonably short (3-6 words)
- Default status is `open`
- Default priority is `normal`
- All images must be in AVIF format — convert PNG/JPG/WebP to AVIF before saving
