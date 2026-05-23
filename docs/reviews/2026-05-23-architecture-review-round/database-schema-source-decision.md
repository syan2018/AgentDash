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

The repository currently has 56 PostgreSQL migration files and broad repository-side PostgreSQL DDL. A static scan found schema statements in 23 PostgreSQL repository files, with the largest clusters in workflow, backend, session, MCP preset, skill asset, workspace, routine, and user directory repositories.

This means the codebase is in a transitional hybrid state. The target contract is migrations-only for PostgreSQL, but removing the existing runtime DDL should be a dedicated cleanup task with integration checks rather than a side effect of unrelated repository work.

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

1. Add a PostgreSQL migration-runner readiness check to the API/bootstrap path if one is not already guaranteed before repository construction.
2. Convert Postgres repository `initialize()` methods from schema creation to either no-op readiness hooks or remove them from startup wiring.
3. Move any repository-only PostgreSQL DDL not represented in migrations into new migration files before deleting runtime DDL.
4. Update integration tests that rely on repository `initialize()` to run migrations or use dedicated test fixtures.
5. Consider migration squash only as a separate pre-release database-baseline task, after confirming no environment depends on the current migration chain.
