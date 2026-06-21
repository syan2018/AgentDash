# Research: session-message-ref-owner-review

- Query: 审计 `SessionMessageRefDto -> MessageRef` 是否属于 request-facing incoming command parsing，是否应作为 Contract Boundary 小尾巴机械迁移到 API/session command mapper。
- Scope: internal
- Date: 2026-06-21

## Findings

### Task / Spec Context

`python ./.trellis/scripts/task.py current --source` 在当前 shell 返回 `Current task: (none)` / `Source: none`；本研究按用户显式指定的父任务 `.trellis/tasks/06-21-contract-boundary-ownership-audit/` 和输出路径执行。

读取的必备材料：

| Path | Description |
| --- | --- |
| `.trellis/workflow.md` | Trellis research 产物必须持久化到任务 `research/`；本 sub-agent 不写业务代码。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md` | Owner rules：API adapter owns route request parsing into application command/read inputs。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md` | 已把 `SessionMessageRefDto -> MessageRef` 初步标成 API adapter/session command mapper migration candidate。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | `agentdash-contracts` 承载 wire DTO；route 需要 application/domain model 时由 API layer owns mapping。 |
| `.trellis/spec/backend/session/architecture.md` | `MessageRef` 是 session runtime/trace/projection 坐标，session 层消费 launch、trace、branch、rollback 事实。 |
| `.trellis/spec/backend/session/context-compaction-projection.md` | `MessageRef` 是 compact/fork/rollback 的精确 projection boundary 坐标。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/prd.md` | 本任务目标是形成可迁移 owner map，避免 use case/read model 与 browser-facing wire DTO 同步演进。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/design.md` | 明确要判断 incoming command DTO -> domain command conversion 是否应离开 contracts crate。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/implement.md` | Phase 2 需要对 incoming command conversion 创建实现任务。 |

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-agent-types/src/model/message.rs:12` | `MessageRef` 内部稳定引用，字段为 `turn_id` / `entry_index`，用于 compact cut boundary、restore 对齐和 branch lineage。 |
| `crates/agentdash-contracts/src/runtime/session.rs:349` | `SessionMessageRefDto` browser-facing fork request DTO value object。 |
| `crates/agentdash-contracts/src/runtime/session.rs:354` | `MessageRef -> SessionMessageRefDto` outbound projection conversion。 |
| `crates/agentdash-contracts/src/runtime/session.rs:363` | `SessionMessageRefDto -> MessageRef` reverse conversion；本次核心审计对象。 |
| `crates/agentdash-contracts/src/runtime/session.rs:374` | `CreateSessionForkRequest`，其 `fork_point_ref` 是 `Option<SessionMessageRefDto>`。 |
| `crates/agentdash-api/src/routes/sessions.rs:747` | `fork_session` route，接收 `Json<CreateSessionForkRequest>`。 |
| `crates/agentdash-api/src/routes/sessions.rs:759` | 唯一实际 reverse call site：`req.fork_point_ref.map(Into::into)` 生成 application `SessionForkRequest.fork_point_ref`。 |
| `crates/agentdash-application/src/session/branching.rs:17` | `SessionForkRequest` application command/read input，字段为 `Option<MessageRef>`。 |
| `crates/agentdash-application/src/session/branching.rs:330` | Application 使用 `MessageRef` 解析 fork point event seq。 |
| `crates/agentdash-application/src/session/branching.rs:609` | `resolve_message_ref_event_seq` 在 projection transcript 中验证 message ref、边界和 turn completion。 |
| `crates/agentdash-application/src/session/context_usage_projection.rs:43` | `SessionProjectionMessageRefReadModel` 是 application projection read model。 |
| `crates/agentdash-api/src/routes/sessions.rs:636` | `session_context_projection_to_response` 将 application read model 映射到 contract response。 |
| `crates/agentdash-api/src/routes/sessions.rs:658` | `SessionProjectionMessageRefReadModel -> SessionProjectionMessageRefResponse` outbound API adapter mapping。 |
| `packages/app-web/src/generated/session-contracts.ts:9` | Generated `CreateSessionForkRequest` 暴露 `fork_point_ref?: SessionMessageRefDto`。 |
| `packages/app-web/src/services/session.ts:89` | Frontend `forkSession` 原样发送 generated request，没有本地 mapper。 |
| `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:169` | Frontend 只展示 projection response 的 `message_ref`，属于 outbound projection 消费。 |

### Call Site Classification

| Call Site / Pattern | Direction | Classification | Owner Judgment |
| --- | --- | --- | --- |
| `crates/agentdash-contracts/src/runtime/session.rs:349` `SessionMessageRefDto` definition | wire shape | Contract DTO | Keep in `agentdash-contracts`; this is generated request shape. |
| `crates/agentdash-contracts/src/runtime/session.rs:354` `impl From<MessageRef> for SessionMessageRefDto` | internal fact -> DTO | Allowed outbound projection | Can stay; narrow structural projection from stable session coordinate to browser wire value. |
| `crates/agentdash-contracts/src/runtime/session.rs:363` `impl From<SessionMessageRefDto> for MessageRef` | DTO -> internal value | Request-facing incoming command parsing | Should migrate out of contracts; used to parse `/sessions/{id}/fork` request into application `SessionForkRequest`. |
| `crates/agentdash-api/src/routes/sessions.rs:759` `req.fork_point_ref.map(Into::into)` | route request -> app command | API adapter mapping | Target owner for reverse mapper. The mapping happens while constructing `SessionForkRequest`. |
| `crates/agentdash-application/src/session/branching.rs:330` / `:609` | internal command processing | Application command logic | Keep application input as `MessageRef`; application owns validation against transcript, fork point boundary and turn completion. |
| `crates/agentdash-application/src/session/context_usage_projection.rs:48` `impl From<MessageRef> for SessionProjectionMessageRefReadModel` | internal value -> app read model | Application read model mapping | Keep; this does not depend on contract DTO. |
| `crates/agentdash-api/src/routes/sessions.rs:658` read model -> `SessionProjectionMessageRefResponse` | app read model -> DTO | API adapter outbound projection | Keep in API route mapper; already matches owner rule. |
| `crates/agentdash-contracts/src/runtime/session.rs:423` `SessionLineageRecord -> SessionLineageRecordResponse` | persistence record -> DTO | Allowed outbound projection / response mapping | Keep or later move with broader lineage response mapper; not part of `SessionMessageRefDto -> MessageRef` incoming issue. It passes `fork_point_ref_json` as stored JSON and does not parse request DTO. |
| `packages/app-web/src/services/session.ts:89` | browser service -> API request | Frontend generated DTO consumption | Keep; frontend uses generated request type and should not own backend `MessageRef` parsing. |

### Decision

`SessionMessageRefDto -> MessageRef` is request-facing incoming command parsing.

The concrete evidence is that the reverse conversion has one backend call site: `crates/agentdash-api/src/routes/sessions.rs:759`, inside `POST /sessions/{id}/fork`, where `CreateSessionForkRequest.fork_point_ref` is converted into application `SessionForkRequest.fork_point_ref`. The resulting `MessageRef` is not a wire value anymore; application branching uses it to resolve a fork boundary, enforce transcript membership, reject incomplete tool-result groups, and require completed turns in `crates/agentdash-application/src/session/branching.rs:609`, `:631`, `:645`.

Therefore the DTO definition should stay contract-owned, the outbound `MessageRef -> SessionMessageRefDto` can stay as narrow projection, but the reverse `impl From<SessionMessageRefDto> for MessageRef` should be removed from `agentdash-contracts` and replaced by a route/application command mapper in `agentdash-api`.

### Minimal Write Set If Migrated

Smallest mechanical migration:

| Path | Change |
| --- | --- |
| `crates/agentdash-contracts/src/runtime/session.rs` | Remove `impl From<SessionMessageRefDto> for MessageRef`; keep `SessionMessageRefDto`, `CreateSessionForkRequest`, and outbound `impl From<MessageRef> for SessionMessageRefDto` if still useful. |
| `crates/agentdash-api/src/routes/sessions.rs` | Add route-local mapper such as `fn session_message_ref_to_application(value: SessionMessageRefDto) -> agentdash_agent_types::MessageRef` or `fn fork_point_ref_to_application(...)`; replace `req.fork_point_ref.map(Into::into)`. |
| `crates/agentdash-api/src/routes/sessions.rs` imports | Import `SessionMessageRefDto` and `agentdash_agent_types::MessageRef` only if the helper is typed explicitly. |
| `crates/agentdash-api/src/routes/sessions.rs` tests | Add or adjust a route mapper unit test if local test scaffolding is cheap; otherwise rely on compile coverage plus existing session branching tests. |

Optional but cleaner follow-up if this grows:

| Path | Change |
| --- | --- |
| `crates/agentdash-api/src/routes/session_mappers.rs` or existing session route mapper module if one exists | Move route-local session DTO mappers out of the large `sessions.rs`; only worth doing if more session command mappers are migrated in the same batch. |

### Route / Application Owner

- Route/API owner: `crates/agentdash-api/src/routes/sessions.rs`, specifically `fork_session`.
- Application owner: `crates/agentdash-application/src/session/branching.rs::SessionForkRequest` and `SessionBranchingService`.
- Contract owner after migration: DTO wire shape only, plus outbound projection if retained.

### Conflict Boundary

- Do not move `MessageRef` itself. It belongs to `agentdash-agent-types` as runtime/projection coordinate.
- Do not change `CreateSessionForkRequest` or generated TypeScript field names; `fork_point_ref?: SessionMessageRefDto` is the browser contract.
- Do not alter application fork validation semantics in `resolve_message_ref_event_seq`, `validate_fork_point_message_boundary`, or `ensure_fork_point_turn_completed`; this task is a mapper ownership move only.
- Do not mix this with broader context usage projection migration; `SessionProjectionMessageRefReadModel` already lives in application and its response mapper already lives in API.
- Be aware that `SessionLineageRecordResponse` still contains `fork_point_ref_json` as stored JSON. That is outbound persistence projection and not the reverse DTO parser under review.

### Suggested Validation Commands

Focused commands:

```powershell
cargo test -p agentdash-api context_projection_mapper_preserves_usage_read_facts
cargo test -p agentdash-application fork_session_materializes_child_initial_projection
cargo test -p agentdash-application projection_view_marks_summary_as_synthetic_projection
pnpm run contracts:check
```

Broader compile check if the mapper import touches crate boundaries:

```powershell
cargo check -p agentdash-api
cargo check -p agentdash-contracts
```

### Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: API layer owns mapping when route needs application/domain model internally.
- `.trellis/spec/backend/session/architecture.md`: `RuntimeSession` owns trace/projection/lineage facts; `MessageRef` participates in runtime restore and branch boundaries.
- `.trellis/spec/backend/session/context-compaction-projection.md`: `MessageRef` is the durable model-context boundary coordinate for compact/fork/rollback.
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md`: incoming route request parsing is API adapter-owned.
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md`: prior owner map already marks this reverse conversion as migration candidate.

### External References

No external references were required. This is an internal ownership and call-site audit.

## Caveats / Not Found

- `task.py current --source` did not resolve an active task in this shell; this file uses the explicit task path supplied in the user prompt.
- No business code, specs, generated contracts, tests, or git state were modified.
- The only concrete `SessionMessageRefDto -> MessageRef` call site found is `crates/agentdash-api/src/routes/sessions.rs:759`.
- No frontend code was found constructing `fork_point_ref` beyond passing the generated `CreateSessionForkRequest` through `forkSession`; current UI appears to consume projection/lineage output more than it actively sends fork-point refs.
- Implementation should stay mechanical. Any change to fork boundary validation belongs to a separate session-branching behavior task.
