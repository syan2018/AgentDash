# WI-08 Presentation And Read-Model Cleanup

Status: done

Assigned Worker: Codex

## Tracking

- Files changed:
  - `crates/agentdash-application/src/agent_run/presentation_read_model.rs`
  - `crates/agentdash-application/src/agent_run/mod.rs`
  - `crates/agentdash-api/src/app_state.rs`
  - `crates/agentdash-api/src/routes/lifecycle_views.rs`
  - `crates/agentdash-api/src/routes/sessions.rs`
- Tests run:
  - `cargo check -p agentdash-api`
- Blockers: None recorded.
- Handoff summary: RuntimeSession trace、AgentFrame runtime view 与 session runtime-control 的 current-frame presentation 拼装已迁入 `AgentRunPresentationReadModelQuery`。API route 保留鉴权、contract DTO 映射和错误映射，既有 response contract 未调整。

## Purpose

Move presentation/current-frame read models behind application query facades so API routes do not assemble anchors/current frames directly.

## Dependencies

- `WI-02`

## Scope

- `routes/sessions.rs` runtime-control view.
- `routes/lifecycle_views.rs` session trace and AgentFrame runtime view.
- Any route-local current frame resolver use that is presentation/read-model only.

## Out Of Scope

- Runtime action/current surface consumers belong to `WI-05`.

## Deliverables

- Application read-model facade for RuntimeSession trace/control-plane view.
- API routes map facade DTOs only.

## Acceptance

- API presentation routes do not import current frame resolver directly.
- Existing response contract remains stable unless explicitly updated.
