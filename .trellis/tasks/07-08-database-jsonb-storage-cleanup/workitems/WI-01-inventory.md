# WI-01 Inventory

Status: done

Scope:

- Enumerate live TEXT JSON columns from migrations and repository mappings.
- Split candidates into `jsonb`, scalar `text`, raw `text`, historical/not-live.

Evidence:

- `research/schema-text-json-inventory.md`
- `research/runtime-workflow-repository-inventory.md`
- `research/product-config-repository-inventory.md`
- `research/text-json-column-inventory.md`

Outcome:

- No PostgreSQL `json` candidate found.
- Live structured document columns are assigned to `jsonb`.
- Scalar enum/string fields are assigned to text.
