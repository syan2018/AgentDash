# Research: Persistence Surface for the SQL Migration

- **Query**: locate `workflow_definitions` schema and access sites; identify column type for `contract`; enumerate migration layout; confirm no other tables store a `capabilities` array for workflow concerns; gather precedent for JSONB path updates.
- **Scope**: internal
- **Date**: 2026-04-22

---

## 1. Schema declaration of `workflow_definitions`

The table is declared in **two places**:

### Migration (Postgres production)

- `crates/agentdash-infrastructure/migrations/0001_init.sql:224–237`

```sql
CREATE TABLE IF NOT EXISTS workflow_definitions (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    binding_kind TEXT NOT NULL,
    recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL,
    status TEXT NOT NULL,
    version INTEGER NOT NULL,
    contract TEXT NOT NULL,        -- ← contract column
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

Later-evolution migrations:
- `0013_workflow_project_scoped.sql:4,17,18` — adds `project_id TEXT NOT NULL`, drops unique-by-key, creates `UNIQUE(project_id, key)` index.
- `0014_workflow_status_nullable.sql:6` — makes `status` nullable.
- `0016_workflow_contract_capabilities.sql` — previous in-flight migration that merges `step.capabilities` into `contract.capabilities` (sets precedent for the current task).

### Runtime CREATE-IF-NOT-EXISTS path

- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:20–28`

```rust
sqlx::query(r#"CREATE TABLE IF NOT EXISTS workflow_definitions (
    id TEXT PRIMARY KEY, project_id TEXT NOT NULL, key TEXT NOT NULL,
    name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
    binding_kind TEXT NOT NULL, recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL, version INTEGER NOT NULL, contract TEXT NOT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
    UNIQUE(project_id, key)
)"#)
```

This runtime-initialize path is the `PostgresWorkflowRepository::initialize` method invoked on startup when the `0001_init.sql` migration has not been run (dev convenience). Its schema must stay in sync with migrations.

---

## 2. Column type for `contract`

**Column type: `TEXT` — NOT `JSONB`.**

Confirmed in both declarations above (`contract TEXT NOT NULL`). The Rust writer side also uses `serde_json::to_string(&workflow.contract)?` before binding (workflow_repository.rs:76) — i.e. serializes to a string and stores as plain text.

To operate on the JSON content in SQL, every migration in this repo **casts at query time** (`contract::jsonb`) and writes back via `::text`. See:

- `0016_workflow_contract_capabilities.sql:45` — `SELECT id, contract::jsonb AS contract_json INTO wf_row`
- `0016_workflow_contract_capabilities.sql:68` — `SET contract = wf_contract::text`
- `0016_workflow_contract_capabilities.sql:13` — inline comment: *"因为 steps / contract 存储为 TEXT（JSON）列，这里用 PL/pgSQL 过程按行处理。"*
- `0012_cleanup_compat_debt.sql:19–33` — same pattern for `stories.context` (TEXT, JSON cast).

**Implication for the new migration**: follow exactly the same cast-to-jsonb / cast-back-to-text pattern. Do not change the column type to real JSONB in this migration.

---

## 3. `migrations/` directory layout

- Location: `crates/agentdash-infrastructure/migrations/`
- Files (sorted):
  ```
  0001_init.sql
  0002_llm_providers.sql
  0003_sessions_bootstrap_state.sql
  0004_sessions_title_source.sql
  0005_backends_owner_user_id.sql
  0006_remove_task_session_id.sql
  0007_routines.sql
  0008_lifecycle_run_session_id.sql
  0009_lifecycle_edges_port_outputs.sql
  0010_inline_fs_files.sql
  0011_agent_link_knowledge_and_containers.sql
  0012_cleanup_compat_debt.sql
  0013_workflow_project_scoped.sql
  0014_workflow_status_nullable.sql
  0015_mcp_presets.sql
  0016_workflow_contract_capabilities.sql
  0017_lifecycle_edge_kind.sql
  ```

- **Highest current number: `0017`**.
- **Next migration filename**: `0018_capability_directives.sql` (or similar; PRD suggests "capability_directives").

Conventions observed:
- Zero-padded 4-digit numeric prefix.
- Lowercase `snake_case` topic.
- Single `.sql` file per migration (no paired `.up.sql` / `.down.sql`); the PRD requires "up + down" — current repo style is to put `-- down:` comments in the same file or to make the `up` idempotent. Precedent for a down section: **none of the existing migrations contain an explicit DOWN block**. The implementer will be setting a new pattern.
- Each migration uses `DO $$ ... BEGIN ... END $$;` PL/pgSQL blocks for row-by-row JSON rewriting (`0012`, `0016`, `0017` all follow this).

---

## 4. Other tables storing `capabilities` arrays

Grep for `"capabilities"` JSONB write/read paths across the persistence crate:

- **`workspaces.mount_capabilities`** (`workspace_repository.rs:74, 183, 186, 213, 233, 260, 264, 367–372, 385`)
  TEXT column storing a `Vec<MountCapability>`. This is **VFS mount capability (read/write/list/search)**, unrelated to workflow capabilities. Do NOT touch in the migration.
- **`context_container` definitions** (domain + frontend context.ts) — also unrelated (filesystem-container capabilities). Skip.
- **`session_workspace` / `agent_binding`** — grepped; neither table persists a `capabilities` key that feeds the workflow capability pipeline.

No other workflow-scoped `capabilities` JSON arrays exist in the persistence layer. The migration only needs to rewrite `workflow_definitions.contract` JSON.

### Builtin JSON fixtures (NOT in DB but still part of Phase 0)

- `crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json:21,38` — two literal `"capabilities": ["workflow_management"]` entries.
- `crates/agentdash-application/src/workflow/builtins/trellis_dag_task.json` — no `"capabilities"` key present (confirmed).

These must be hand-edited to `"capability_directives": [{"Add":"workflow_management"}]` as part of the same PR.

---

## 5. SQLx / PL-pgSQL precedent for JSONB path updates

The closest relative of the new migration is `0016_workflow_contract_capabilities.sql`. Key primitives already demonstrated in this repo:

### Cast + read

```sql
SELECT id, contract::jsonb AS contract_json INTO wf_row
FROM workflow_definitions
WHERE project_id = lc.project_id AND key = wk
LIMIT 1;

wf_contract := COALESCE(wf_row.contract_json, '{}'::jsonb);
existing_caps := COALESCE(wf_contract -> 'capabilities', '[]'::jsonb);
```

### Rewrite JSONB field (`jsonb_set`)

```sql
wf_contract := jsonb_set(wf_contract, '{capabilities}', merged_caps, true);
```

### Walk array elements

```sql
FOR directive IN SELECT * FROM jsonb_array_elements(step_caps) LOOP
    add_key := directive ->> 'add';
    ...
END LOOP;
```

### Strip a key

```sql
new_step := step_item - 'capabilities';   -- jsonb subtract operator
```

### Write back as TEXT

```sql
UPDATE workflow_definitions
SET contract = wf_contract::text,
    updated_at = NOW()::TEXT
WHERE id = wf_row.id;
```

### Build replacement array iteratively

```sql
new_steps := '[]'::jsonb;
...
new_steps := new_steps || new_step;
```

### Row-level loop scaffolding

```sql
DO $$
DECLARE
    lc RECORD;
    ...
BEGIN
    FOR lc IN SELECT id, project_id, steps::jsonb AS steps_json
              FROM lifecycle_definitions LOOP
        IF jsonb_typeof(lc.steps_json) <> 'array' THEN
            CONTINUE;
        END IF;
        ...
    END LOOP;
END $$;
```

### Type-guard pattern (avoid noisy errors)

```sql
IF jsonb_typeof(step_caps) <> 'array' THEN CONTINUE; END IF;
IF NOT (existing_caps @> to_jsonb(add_key)) THEN ... END IF;
```

### Precedent for "fallback kind" on missing keys (analogous to the new migration's alias-expansion)

`0017_lifecycle_edge_kind.sql:48` — `patched_edge := jsonb_set(edge_item, '{kind}', '"artifact"'::jsonb, true);` — inserting a literal JSON string value at a path.

### Conditional rewrite pattern for non-uniform array elements

`0016_workflow_contract_capabilities.sql:44–82`:

```sql
IF wk IS NOT NULL AND step_caps IS NOT NULL AND jsonb_typeof(step_caps) = 'array' THEN
    -- branch A: merge
ELSE
    -- branch B: skip
END IF;
new_step := step_item - 'capabilities';
new_steps := new_steps || new_step;
```

This will be the direct template for rewriting each old `capabilities` entry — `jsonb_typeof` dispatches on `string` vs `object`, then the implementer builds a replacement `capability_directives` array per the mapping table in the PRD §"一次性迁移规则".

### `CASE WHEN` usage

No existing migration uses `CASE WHEN` on JSONB — everything is done with `IF ... THEN` inside `DO $$ ... $$` blocks. Stick with `IF ... THEN` for consistency.

### SQLx (Rust side) — no direct SQL string rewrite for this

The Rust `workflow_repository.rs` reads `contract` as a TEXT column and round-trips it through `serde_json::from_str`. The migration is therefore a **pure SQL operation**; the Rust code does **not** need any migration-related helper, just the new domain types (which will read the new `capability_directives` field via serde after the SQL has run).

---

## Caveats / Not Found

- **No existing migration has a down block.** The PRD calls for a reversible migration (up + down) — the implementer is establishing a new convention here. Options: (1) put `-- ── down: capability_directives → capabilities ──` section inside the same file guarded by a comment, (2) split to `0018_..._up.sql` + `0018_..._down.sql`. No precedent in this repo; align with PRD Acceptance Criteria.
- **No SQLite-side migration exists for this repo's workflow_definitions.** `crates/agentdash-infrastructure/src/persistence/sqlite/` has only `mod.rs` and `session_repository.rs` — no workflow-definition SQLite table. The Postgres migration is the sole persistence migration surface.
- **`jsonb_set` with missing path default**: existing migrations pass `true` as the 4th argument, meaning "create if missing". That's the right default for the new migration too.
- **`NOW()::TEXT` pattern**: used verbatim in migrations for `updated_at`. Follow this precedent exactly — not `CURRENT_TIMESTAMP`, not `now()::text`.
- **PG-version assumption**: all JSONB operators used (`->`, `->>`, `@>`, `||`, `-`, `jsonb_set`, `jsonb_array_elements`, `jsonb_typeof`, `to_jsonb`) are PostgreSQL 9.5+ features; no version gate exists in current migrations.
