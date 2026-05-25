# Database Schema Source Decision

## Decision

AgentDash uses separate schema source contracts for cloud business data and local session cache.

| Store | Source of Truth | Why |
| --- | --- | --- |
| PostgreSQL cloud business database | `crates/agentdash-infrastructure/migrations/*.sql` | Cloud business data needs an auditable, ordered schema history that is identical in development, tests, embedded Postgres, and deployed environments. |
| SQLite local session cache | `SqliteSessionRepository::initialize()` | The local cache is owned by the desktop/local runtime, is created per user environment, and does not share the cloud database migration lifecycle. |
| Memory persistence | Rust structs/tests | In-memory stores model behavior and do not own durable schema. |

PostgreSQL repository implementations should treat schema as already migrated. Repository startup is still useful for wiring validation, lightweight seed orchestration, and future schema readiness checks, but new PostgreSQL tables, columns, indexes, constraints, and drops belong in migrations.

SQLite keeps its lightweight initialization path because it is a local runtime store rather than the cloud business database. Its `initialize()` can create tables and apply additive local cache changes while preserving idempotent startup.

## Current Baseline

The repository currently has 56 PostgreSQL migration files. PostgreSQL runtime DDL has been removed from repository implementations, and API repository bootstrap now verifies migrated schema readiness before constructing the repository set.

This means PostgreSQL schema ownership is aligned with the target contract: migrations are the cloud business database source of truth, while repository code assumes migrated tables and indexes already exist.

## PostgreSQL Migration Rules

- New PostgreSQL schema changes are represented by a new numbered migration file.
- Existing migration files remain stable once committed, so schema history stays reproducible.
- Repository SQL assumes migrated schema and focuses on aggregate persistence behavior.
- Embedded/local development Postgres uses the same migration runner as other PostgreSQL environments.
- PostgreSQL integration tests should initialize schema through migrations when they need real tables.

## SQLite Local Cache Rules

- Local SQLite session cache initialization remains inside `SqliteSessionRepository::initialize()`.
- SQLite cache schema changes should stay idempotent and scoped to local session metadata, events, terminal effects, and runtime command cache.
- SQLite rules should not be copied back into cloud PostgreSQL repository patterns.

## Cleanup Checklist

1. Keep PostgreSQL schema changes in new numbered migration files.
2. Keep API/bootstrap schema checks as readiness validation, not schema mutation.
3. Keep PostgreSQL integration tests on migration-initialized schema fixtures.
4. Consider migration squash only as a separate pre-release database-baseline task, after confirming no environment depends on the current migration chain.
