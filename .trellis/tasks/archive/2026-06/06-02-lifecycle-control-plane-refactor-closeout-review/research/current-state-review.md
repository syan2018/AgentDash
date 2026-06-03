# Current State Review Evidence

Date: 2026-06-02
Branch: `codex/refactor-lifecycle-control-plane`

## Verdict

当前重构不能算妥善完成。它已经完成了主体结构迁移，但还没有收束到可验证的薄架构边界。

## Positive Evidence

- `RuntimeLaunchRequest` struct 已删除，`workflow/runtime_launch.rs` 当前只保留 `FrameLaunchEnvelope` / frame runtime surface。
- `FrameConstructionService` 已在 `crates/agentdash-application/src/workflow/frame_construction/` 落地。
- API `AppStateSessionConstructionProvider` 已退化为调用 application 层 construction service 的 thin wrapper。
- `AgentFrameSurfaceExt` 已提供 typed surface accessor。
- dispatch result DTO 已引入 `delivery_runtime_ref`。
- `RuntimeSessionExecutionAnchor` domain 类型、repository trait、Postgres repository 已存在。
- 前端部分组件已经优先读取 `agent.delivery_runtime_ref.runtime_session_id`。

## Blocking Findings

### 1. Domain test compile failure

Command:

```bash
cargo test -p agentdash-domain --lib -- --format terse
```

Result:

```text
error[E0560]: struct `dispatch::SubjectExecutionDispatchResult` has no field named `runtime_session_ref`
  --> crates/agentdash-domain/src/workflow/dispatch.rs:285:13

error[E0560]: struct `dispatch::SubjectExecutionDispatchResult` has no field named `trace_ref`
  --> crates/agentdash-domain/src/workflow/dispatch.rs:286:13
```

Evidence:

- `crates/agentdash-domain/src/workflow/dispatch.rs:186` defines `AgentLaunchDispatchResult.delivery_runtime_ref`.
- `crates/agentdash-domain/src/workflow/dispatch.rs:197` defines `SubjectExecutionDispatchResult.delivery_runtime_ref`.
- `crates/agentdash-domain/src/workflow/dispatch.rs:285` and `:286` still construct old fields in a test.

Impact:

The branch fails its own test compilation gate and cannot be considered complete.

### 2. RuntimeSessionExecutionAnchor writes incorrect activity key

Evidence:

- `crates/agentdash-application/src/workflow/dispatch_service.rs:423` creates `RuntimeSessionExecutionAnchor::new_dispatch`.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:429` writes `Some("entry".to_string())`.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:671` creates frame scope from `workflow_graph.entry_activity_key.clone()`.

Impact:

For a graph whose entry activity key is not literally `entry`, anchor evidence records the wrong activity. The second-stage assignment update only writes `assignment_id` and `attempt`, so the wrong `activity_key` persists.

### 3. Application lib tests have a frame projection failure

Command:

```bash
cargo test -p agentdash-application --lib -- --format terse
```

Result:

```text
test result: FAILED. 671 passed; 1 failed

canvas::tools::tests::present_canvas_updates_meta_capability_skill_and_events
thread panicked at crates/agentdash-application/src/canvas/tools.rs:1349:9:
assertion `left == right` failed
  left: []
 right: ["demo"]
```

Evidence:

- `crates/agentdash-application/src/canvas/tools.rs:1334` executes `present_canvas`.
- `crates/agentdash-application/src/canvas/tools.rs:1344` reads the updated frame via `find_by_runtime_session(&session.id)`.
- `crates/agentdash-application/src/canvas/tools.rs:1349` expects `visible_canvas_mount_ids()` to be `["demo"]`.

Impact:

The frame-based projection used after a runtime-session-addressed canvas tool call is not satisfying the existing capability/mount update test. This is a concrete application-level regression signal, not only a naming or cleanup issue.

### 4. Anchor is not yet authoritative resolver evidence

Evidence:

- `crates/agentdash-application/src/workflow/session_association.rs:118` resolves by runtime session.
- `crates/agentdash-application/src/workflow/session_association.rs:124` calls `frame_repo.find_by_runtime_session(session_id)`.
- `crates/agentdash-application/src/workflow/session_association.rs:138` then selects assignment from current frame.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:510` implements `find_by_runtime_session`.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:527` uses anchor row only to parse `agent_id`.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:536` queries latest frame by `agent_id`.

Impact:

The resolver still effectively reconstructs business association through current frame state. Anchor `assignment_id` is not consumed as primary evidence.

### 5. Deprecated construction concepts remain public production surface

Evidence:

- `crates/agentdash-application/src/session/mod.rs:13` exports `pub mod construction`.
- `crates/agentdash-application/src/session/mod.rs:14` exports `pub mod construction_planner`.
- `crates/agentdash-application/src/session/mod.rs:16` exports `pub mod construction_use_case`.
- `crates/agentdash-application/src/session/construction.rs:26` defines `ResolvedSessionOwner`.
- `crates/agentdash-application/src/session/construction.rs:74` defines `RuntimeContextInspectionPlan`.
- `crates/agentdash-application/src/session/construction_use_case.rs:57` exposes `finalize_session_construction_projection`.

Validation:

```bash
cargo check -p agentdash-application
```

Result:

```text
warning: `agentdash-application` (lib) generated 48 warnings
```

The warning cluster is all deprecated construction type usage.

Impact:

Phase 5 is not complete. The old large plan object can still leak into new code and remains visible in production crate compilation.

### 6. Frontend still derives primary runtime session from run trace refs

Evidence:

- `packages/app-web/src/stores/lifecycleStore.ts:74` still declares `primarySessionId`.
- `packages/app-web/src/stores/lifecycleStore.ts:276` returns `run?.runtime_trace_refs[0]?.runtime_session_id`.
- `packages/app-web/src/components/layout/SessionShortcutList.tsx:91` computes `primarySessionId`.
- `packages/app-web/src/components/layout/SessionShortcutList.tsx:93` falls back to `run.runtime_trace_refs[0]?.runtime_session_id`.
- `packages/app-web/src/features/agent/active-session-list.tsx:323` computes `primarySessionId`.
- `packages/app-web/src/features/agent/active-session-list.tsx:325` falls back to `run.runtime_trace_refs[0]?.runtime_session_id`.
- `packages/app-web/src/types/session.ts:108` still declares `HookSessionRuntimeInfo`.

Validation:

```bash
pnpm --filter app-web run typecheck
```

Result: passed.

Impact:

Type correctness is fine, but read-model semantics are not fully agent/frame-first.

### 7. port output map still uses run-level fallback in compose

Evidence:

- `crates/agentdash-application/src/workflow/execution_log.rs:191` defines `load_scoped_port_output_map`.
- `crates/agentdash-application/src/workflow/execution_log.rs:215` defines run-level `load_port_output_map`.
- `crates/agentdash-application/src/session/assembler.rs:1287` has TODO `phase-6e` and calls run-level `load_port_output_map`.
- `crates/agentdash-application/src/session/assembler.rs:1616` repeats the same TODO and run-level call.

Impact:

Activity output remains available through a run-wide query path at frame compose time.

### 8. SessionMeta still carries launch facts

Evidence:

- `crates/agentdash-application/src/session/construction_provider.rs:62` defines `SessionConstructionProviderInput`.
- `crates/agentdash-application/src/session/construction_provider.rs:65` includes full `SessionMeta`.
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs:153` defines `prompt_lifecycle`.
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs:165` passes `&input.session_meta` to lifecycle resolution.
- `crates/agentdash-application/src/session/launch/planner.rs:199` uses `session_meta.executor_session_id` as follow-up session source.

Impact:

SessionMeta is still partly a launch decision carrier rather than trace-only runtime metadata.

## Validation Commands Run

```bash
cargo check -p agentdash-application
cargo check --workspace
cargo test -p agentdash-domain --lib -- --format terse
cargo test -p agentdash-application --lib -- --format terse
pnpm --filter app-web run typecheck
```

Results:

- `cargo check -p agentdash-application`: passed with 48 deprecated warnings.
- `cargo check --workspace`: passed with the same deprecated warning cluster.
- `cargo test -p agentdash-domain --lib -- --format terse`: failed due stale dispatch DTO test fields.
- `cargo test -p agentdash-application --lib -- --format terse`: failed with 671 passed / 1 failed; canvas present tool does not update expected frame visible canvas mount ids.
- `pnpm --filter app-web run typecheck`: passed.

## Review Conclusion

The branch is structurally close, but not complete. The remaining work should be handled as final convergence, not as compatibility polish.
