# Domain Docs

## Layout

Single-context: one `CONTEXT.md` at repo root + `docs/adr/` for architectural decisions.

## Consumer rules

- **CONTEXT.md** — Read this first when entering the repo. Contains domain language, key concepts, and project overview.
- **docs/adr/** — Read ADRs when investigating why a decision was made. Files named `NNNN-title.md`.

## Creating ADRs

Use the format:
```markdown
# ADR-NNNN: Title

## Status
Proposed / Accepted / Deprecated / Superseded by ADR-NNNN

## Context
What problem are we solving?

## Decision
What did we decide?

## Consequences
What are the trade-offs?
```
