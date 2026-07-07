# WI-02 Migration

Status: done

Scope:

- Add forward migration `0058_json_text_columns_to_jsonb.sql`.
- Convert live structured TEXT JSON columns to `jsonb`.
- Preserve scalar text columns and normalize legacy JSON-string enum values.

Files:

- `crates/agentdash-infrastructure/migrations/0058_json_text_columns_to_jsonb.sql`

Validation:

- `pnpm run migration:guard` passed.
