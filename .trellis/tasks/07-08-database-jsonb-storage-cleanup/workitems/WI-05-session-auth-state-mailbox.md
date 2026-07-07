# WI-05 Session, Auth, State, Mailbox

Status: done

Scope:

- Convert PostgreSQL session persistence JSON columns from string JSON helpers to `serde_json::Value` / typed `from_value` mapping.
- Convert auth session identity, state change payload, mailbox source metadata, and agent-run lineage refs/metadata to JSONB typed mapping.

Files:

- `session_repository.rs`
- `session_core.rs`
- `auth_session_repository.rs`
- `state_change_store.rs`
- `agent_run_mailbox_repository.rs`
- `agent_run_lineage_repository.rs`
- `crates/agentdash-domain/src/auth_session/entity.rs`
- `crates/agentdash-application/src/auth/session_service.rs`

Validation:

- `cargo check -p agentdash-infrastructure` passed.
- Static grep for JSON string helpers in PostgreSQL persistence is empty.
