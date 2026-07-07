# WI-04 Workflow And Runtime Repositories

Status: done

Scope:

- Convert workflow/lifecycle documents and lifecycle anchor metadata/payload columns to JSONB typed mapping.
- Keep lifecycle/workflow scalar enum fields as text.
- Remove JSONB casts from queries once columns are native JSONB.

Files:

- `workflow_repository.rs`
- `lifecycle_anchor_repository.rs`
- `agent_run_lineage_repository.rs`

Validation:

- `cargo check -p agentdash-infrastructure` passed.
