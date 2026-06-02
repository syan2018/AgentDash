# Research: scoped lifecycle artifacts

- Query: 当前 run-level `port_outputs` 如何影响多 `graph_instance` 和同名 port？目标 scoped artifact 存储应该影响哪些文件和测试？
- Scope: internal
- Date: 2026-06-02

## Findings

### Files found

- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/prd.md` — 父任务需求，明确要求 lifecycle artifact / output port 存储改为 `graph_instance_id + activity_key + attempt + port_key` scoped。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/design.md` — 目标设计，声明 run-level artifact view 只能从 scoped artifact 聚合生成。
- `.trellis/tasks/06-02-lifecycle-control-plane-frame-convergence/implement.md` — Phase 2 执行计划，列出 scoped lifecycle artifacts 对 VFS、journey、hook gate、completion policy、artifact binding、migration、测试的影响。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` — Activity runtime identity 与 artifact contract：`WorkflowGraphInstance.activity_state` 是事实源，output port 内容必须是 JSON。
- `.trellis/spec/backend/workflow/architecture.md` — Workflow 子系统不变量：Activity / attempt runtime key 必须包含 `graph_instance_id`。
- `.trellis/spec/backend/workflow/lifecycle-edge.md` — artifact edge 隐含 flow dependency，edge 层只表达端口依赖，不应承载运行态 artifact scope。
- `.trellis/spec/backend/vfs/architecture.md` — VFS runtime mount 是 provider 分发单位，inline storage 坐标只能由 application resolver 从 mount metadata 生成。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` — hook runtime 目标是 frame-first，workflow/hook policy authority 来自 active workflow projection / effective contract。
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` — Activity state 已有 `graph_instance_id`，但 `ActivityOutputArtifact` / `ActivityInputArtifact` 自身还只含 activity/attempt/port。
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs` — `WorkflowGraphInstance` 持有 scoped `activity_state`，并校验 `activity_state.graph_instance_id == instance.id`。
- `crates/agentdash-domain/src/workflow/entity.rs` — `LifecycleRun.active_node_keys` 是从 graph instance activity state 派生的 run-level projection。
- `crates/agentdash-application/src/vfs/mount.rs` — lifecycle mount 已携带 `run_id` 与 `graph_instance_id` metadata，但 writable port 白名单仍只是 port key 列表。
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` — lifecycle VFS provider 从 mount metadata 加载 active graph instance，但 artifacts read/write 仍调用 run-level journey port output API。
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs` — journey port output API 以 `LifecycleRun` inline file owner + `port_outputs` container + `port_key` path 读写。
- `crates/agentdash-application/src/workflow/execution_log.rs` — `load_port_output_map` 统一加载 run-level `port_outputs`，是 hook / assembler / orchestrator 的共同入口。
- `crates/agentdash-application/src/workflow/orchestrator.rs` — `complete_lifecycle_node` 用 run-level port map 做 completion gate 和 ActivityCompleted outputs。
- `crates/agentdash-application/src/hooks/provider.rs` — hook snapshot 的 `fulfilled_port_keys` 来自 run-level port map。
- `crates/agentdash-application/scripts/hook-presets/port_output_gate.rhai` — BeforeStop gate 只比较 required port keys 与 `fulfilled_port_keys`，因此继承 run-level 污染。
- `crates/agentdash-application/src/workflow/activity_activation.rs` — kickoff prompt 渲染 `lifecycle://artifacts/{port}`，并用 caller 传入的 ready port key 集合标记 input readiness。
- `crates/agentdash-application/src/session/assembler.rs` — lifecycle session assembly / companion slice 都用 run-level port map 推导 `ready_port_keys`。
- `crates/agentdash-application/src/workflow/projection.rs` — `ActiveWorkflowProjection` 已包含 run、graph_instance_id、active_activity、active_attempt，可作为 scoped artifact 查询的自然输入。
- `crates/agentdash-infrastructure/migrations/0010_inline_fs_files.sql` — inline file 唯一键为 `(owner_kind, owner_id, container_id, path)`，当前可通过 container/path 编码 artifact scope。
- `crates/agentdash-infrastructure/migrations/0073_lifecycle_target_anchors.sql` — 现有 target anchor schema 已为 `agent_assignments(graph_instance_id, activity_key, attempt)` 建索引。
- `crates/agentdash-infrastructure/migrations/0084_lifecycle_control_plane_hard_cutover.sql` — 已删除 `lifecycle_runs.port_outputs` 列；当前 run-level port output 事实实际留在 inline files 中。
- `packages/app-web/src/generated/workflow-contracts.ts` / `crates/agentdash-contracts/src/workflow.rs` — contract DTO 中 `ActivityOutputArtifact` / `ActivityInputArtifact` 仍未包含 `graph_instance_id` 字段。

### Current run-level port output behavior

当前 VFS mount 已经带有 graph scope：`build_lifecycle_mount_with_ports` 写入 `root_ref = lifecycle://run/{run_id}/graph/{graph_instance_id}`，metadata 同时包含 `run_id`、`graph_instance_id`、`lifecycle_key` 与 `writable_port_keys`（`crates/agentdash-application/src/vfs/mount.rs:832`、`crates/agentdash-application/src/vfs/mount.rs:849`、`crates/agentdash-application/src/vfs/mount.rs:853`）。VFS provider 也会用 metadata 加载当前 graph instance（`crates/agentdash-application/src/vfs/provider_lifecycle.rs:107`、`crates/agentdash-application/src/vfs/provider_lifecycle.rs:112`、`crates/agentdash-application/src/vfs/provider_lifecycle.rs:119`）。

但是 artifacts 的实际 read/write 没有使用 graph/activity/attempt scope。`LifecycleJourneyService::write_port_output` 固定写入 `InlineFileOwnerKind::LifecycleRun`、`owner_id = run_id`、`container_id = "port_outputs"`、`path = port_key`（`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:296`、`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:302`、`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:305`）。`read_port_output` 和 `list_port_outputs` 同样只用 run id + port key/container（`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:261`、`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:280`）。

`load_port_output_map` 是跨链路的核心污染源：它只接受 `run_id`，返回 `port_key -> content` 的全 run map（`crates/agentdash-application/src/workflow/execution_log.rs:166`、`crates/agentdash-application/src/workflow/execution_log.rs:171`、`crates/agentdash-application/src/workflow/execution_log.rs:176`）。因此同一个 `LifecycleRun` 内任意 graph instance、任意 activity attempt 写入同名 `result` / `report` / `summary` port，都会覆盖或复用同一个 inline file path。

`complete_lifecycle_node` 的 completion gate 用这个 run-level map 检查 required ports：只要 map 里存在同名 key，当前 activity 就被认为已交付，即使该 key 来自另一个 graph instance 或旧 attempt（`crates/agentdash-application/src/workflow/orchestrator.rs:243`、`crates/agentdash-application/src/workflow/orchestrator.rs:250`、`crates/agentdash-application/src/workflow/orchestrator.rs:268`）。随后 `activity_outputs_from_port_map` 只按当前 activity 声明的 output port key 过滤并解析 JSON，没有 graph/activity/attempt 校验（`crates/agentdash-application/src/workflow/orchestrator.rs:436`、`crates/agentdash-application/src/workflow/orchestrator.rs:446`、`crates/agentdash-application/src/workflow/orchestrator.rs:450`）。

Hook gate 也会被同样污染。`ExecutionHookProvider` 构造 workflow snapshot 时从 run-level `load_port_output_map(workflow.run.id)` 填充 `fulfilled_port_keys`（`crates/agentdash-application/src/hooks/provider.rs:198`、`crates/agentdash-application/src/hooks/provider.rs:211`、`crates/agentdash-application/src/hooks/provider.rs:220`），`port_output_gate.rhai` 只比较 `ctx.workflow.output_port_keys` 与 `ctx.workflow.fulfilled_port_keys`（`crates/agentdash-application/scripts/hook-presets/port_output_gate.rhai:12`、`crates/agentdash-application/scripts/hook-presets/port_output_gate.rhai:17`、`crates/agentdash-application/scripts/hook-presets/port_output_gate.rhai:24`）。结果是 BeforeStop 可能因为其它 graph/attempt 的同名 port 而放行，也可能因为 run-level map 被覆盖成无效内容而阻断。

Agent kickoff / input readiness 也继承 run-level 口径。`ActivityActivationInput.ready_port_keys` 注释明确要求调用方提前通过 `load_port_output_map` 查询（`crates/agentdash-application/src/workflow/activity_activation.rs:74`），session assembler 在 lifecycle node 与 companion workflow assembly 中都用 `spec.run.id` 加载全 run map 并转成 ready key 集合（`crates/agentdash-application/src/session/assembler.rs:1287`、`crates/agentdash-application/src/session/assembler.rs:1614`）。因此 input port 的“已就绪/未就绪”提示可能被同名 port 跨 graph instance 误标。

Domain engine 已经更接近目标状态：`ActivityLifecycleRunState` 拥有 `graph_instance_id`（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:67`），`WorkflowGraphInstance::replace_activity_state` 校验 state 与 instance id 一致（`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:57`、`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:61`）；engine 在 `ActivityCompleted` 时把 output artifact 写入当前 state 的 `outputs`（`crates/agentdash-application/src/workflow/engine.rs:303`），artifact binding 从同一个 state 内的 latest output 派生 inputs（`crates/agentdash-application/src/workflow/engine.rs:454`、`crates/agentdash-application/src/workflow/engine.rs:547`）。当前断层在于 VFS/hook/orchestrator completion 使用 run-level inline map 生成 `ActivityCompleted`，而不是从 scoped active attempt artifact store 读取。

### How run-level port_outputs affects multi graph_instance and same-name ports

1. 多 graph instance 同名 port 会共享同一个 inline file row。当前唯一键是 `(owner_kind, owner_id, container_id, path)`（`crates/agentdash-infrastructure/migrations/0010_inline_fs_files.sql:12`），现有写法使所有 graph instances 都落在 `(lifecycle_run, run_id, port_outputs, port_key)`。同名 port 没有 graph namespace。

2. 后写覆盖先写。Postgres inline file repository 对同一唯一键执行 upsert（`crates/agentdash-infrastructure/src/persistence/postgres/inline_file_repository.rs:177`、`crates/agentdash-infrastructure/src/persistence/postgres/inline_file_repository.rs:181`），所以第二个 graph instance 写 `artifacts/report` 会覆盖第一个 graph instance 的 `report`。

3. Completion gate 会误判。`required_ports` 只检查 key 是否存在于全 run map（`crates/agentdash-application/src/workflow/orchestrator.rs:246`、`crates/agentdash-application/src/workflow/orchestrator.rs:252`），因此 Graph B 的 `report` 可让 Graph A 的同名 required port 通过；旧 attempt 的同名 port 也可让新 attempt 通过。

4. JSON 解析错误可能跨 scope 传播。`activity_outputs_from_port_map` 对当前 activity 声明的同名 port 解析 JSON（`crates/agentdash-application/src/workflow/orchestrator.rs:446`、`crates/agentdash-application/src/workflow/orchestrator.rs:450`）。另一个 graph/attempt 写入的非 JSON 或不同 schema 内容会让当前 activity completion 报错，或者把错误值记录为当前 attempt output。

5. Hook gate 与 VFS listing 都呈现 run 聚合而非 active scope。`lifecycle://active/artifacts` 与 `lifecycle://artifacts` 都列出 run-level map（`crates/agentdash-application/src/vfs/provider_lifecycle.rs:233`、`crates/agentdash-application/src/vfs/provider_lifecycle.rs:540`），BeforeStop gate 也只看 run-level fulfilled keys，无法区分 active graph/activity/attempt。

6. Artifact binding 当前在 engine state 内是 graph-instance scoped，但它收到的 source outputs 可能已经在 completion 入口被污染。换言之，engine 的 internal binding 逻辑不是主要问题；问题在 `ActivityCompleted.outputs` 的来源。

### Target scoped artifact storage impact

Recommended storage identity:

```text
run_id
  graph_instance_id
  activity_key
  attempt
  port_key
```

Implementation can either introduce a first-class repository/table for lifecycle activity artifacts, or continue using `inline_fs_files` temporarily with scoped path/container. Given the parent task asks for target state and no compatibility layer, a first-class artifact repository is cleaner if this becomes durable control-plane data. If inline storage remains the physical backing, the minimum safe coordinate is still owner `LifecycleRun`, container such as `activity_port_outputs`, and path such as `{graph_instance_id}/{activity_key}/{attempt}/{port_key}`; runtime API should expose this as typed scope, not as raw string concatenation.

Files that should change:

- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` / `crates/agentdash-contracts/src/workflow.rs` / generated TS contracts: decide whether `ActivityOutputArtifact` / `ActivityInputArtifact` carry `graph_instance_id` explicitly. Because `ActivityLifecycleRunState` already owns graph scope, internal domain structs can omit duplication, but API/read-model DTOs should include it when artifacts are aggregated across graph instances.
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`: replace `list_port_outputs(run_id)`, `read_port_output(run_id, port_key)`, `write_port_output(run_id, port_key, content)` with scoped APIs using `graph_instance_id + activity_key + attempt + port_key`; keep any run-level list as explicit aggregate read model.
- `crates/agentdash-application/src/workflow/execution_log.rs`: replace `load_port_output_map(repo, run_id)` with a scoped loader such as `load_attempt_port_output_map(repo, run_id, graph_instance_id, activity_key, attempt)`. Existing callers should pass `ActiveWorkflowProjection` / assignment / claim facts rather than raw run id.
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`: artifacts read/list/write should derive active `activity_key` and `attempt` from `load_active_context` / current attempt before touching storage. `lifecycle://active/artifacts/{port}` should be active-attempt scoped. If `lifecycle://artifacts` remains, it should be documented/implemented as scoped to the mount's graph and active attempt, or split into an explicit aggregate path.
- `crates/agentdash-application/src/vfs/mount.rs`, `crates/agentdash-application/src/workflow/activity_activation.rs`, `crates/agentdash-application/src/workflow/lifecycle/mount.rs`, `crates/agentdash-application/src/session/assembly_builder.rs`: mount metadata currently has graph id plus writable port keys; scoped writes also need reliable active activity/attempt resolution. This can be resolved dynamically from `WorkflowGraphInstance.activity_state`, or projected into the mount/frame envelope if each launched frame is attempt-specific.
- `crates/agentdash-application/src/workflow/orchestrator.rs`: `complete_lifecycle_node` should load outputs for the resolved association's exact `graph_instance_id + activity_key + attempt`, then create `ActivityCompleted` outputs from that scoped map.
- `crates/agentdash-application/src/hooks/provider.rs` and `crates/agentdash-application/scripts/hook-presets/port_output_gate.rhai`: snapshot `fulfilled_port_keys` must be scoped to active projection (`run + graph_instance_id + active_activity + active_attempt`). Script shape can stay key-based if provider guarantees scope.
- `crates/agentdash-application/src/session/assembler.rs` and `crates/agentdash-application/src/workflow/activity_activation.rs`: `ready_port_keys` is insufficient for artifact bindings when multiple predecessors or graph instances can have same port names. Prefer scoped input artifacts from `ActivityLifecycleRunState.inputs`, or expose readiness per declared input binding rather than global port key.
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`: run/subject artifact projection should aggregate graph instance scoped outputs; it currently already has helper surface around graph instances and `graph_instances_outputs_json`, so it is a likely read-model update point.
- `crates/agentdash-infrastructure/migrations/0010_inline_fs_files.sql` / new migration: no old runtime compatibility path is required, but storage needs either new `lifecycle_activity_artifacts` table or migration of existing inline rows into scoped coordinates. Since old rows lack graph/activity/attempt, migration can only map data when there is a single unambiguous active/root attempt; otherwise the pre-release project can drop or rebuild these rows.
- `crates/agentdash-infrastructure/src/persistence/postgres/inline_file_repository.rs` only needs changes if inline storage remains the physical backing and existing repository API is too generic for scoped artifact operations. A dedicated repository can avoid spreading container/path conventions.
- `packages/app-web/src/generated/workflow-contracts.ts`, `packages/app-web/src/types/workflow.ts`, `packages/app-web/src/services/workflow.ts`, `packages/app-web/src/stores/workflowStore.ts`: generated/typed artifact contracts and frontend run views should represent graph-scoped artifacts when displaying aggregated lifecycle data. The editor's `ArtifactBinding` definition remains a graph definition concern, but run views should not imply run-wide port identity.

Tests that should change or be added:

- Backend unit: `LifecycleJourneyService` or new artifact repository should prove two graph instances in the same run can both write `report` without overwrite, and that attempt 2 does not see attempt 1 outputs unless an explicit alias policy asks for it.
- Backend VFS provider: extend `lifecycle_vfs_records_and_artifacts_are_writable_by_path_rules` so two mounts with same run id but different graph ids write/read same port key independently; current test only verifies one graph mount and one `report` path (`crates/agentdash-application/src/vfs/provider_lifecycle.rs:1653`、`crates/agentdash-application/src/vfs/provider_lifecycle.rs:1701`).
- Orchestrator: add a `complete_lifecycle_node` test where another graph instance or previous attempt has the required port but the active attempt does not; gate must reject. Add the inverse where active attempt has the port and another graph has invalid JSON; completion must ignore the other graph.
- Hook provider / Rhai gate: add snapshot test proving `fulfilled_port_keys` reflects only active `graph_instance_id + activity_key + attempt`.
- Session assembler / activity activation: add prompt/input readiness test where same run has a ready `proposal` in Graph A but Graph B's input should remain unready unless its own transition/input artifact binding has been materialized.
- Engine tests around artifact binding should remain but may need DTO/read-model assertions with `graph_instance_id` when outputs are aggregated; existing engine tests already cover latest output and artifact binding within a single state.
- Infrastructure migration/repository tests: if adding a first-class artifact table, test uniqueness on `(run_id, graph_instance_id, activity_key, attempt, port_key)`. If using inline files, test path encoding through repository-level helpers rather than raw string path assembly in callers.
- Contract generation/tests: update `contracts:check` expected TS output if `ActivityOutputArtifact` / `ActivityInputArtifact` / read views gain `graph_instance_id`.

## Caveats / Not Found

- I did not find a first-class lifecycle artifact repository/table. Current persisted port output storage appears to be inline files under `InlineFileOwnerKind::LifecycleRun`, `container_id = "port_outputs"`.
- `lifecycle_runs.port_outputs` exists in migration `0009` but is dropped by `0084`; the active runtime path no longer depends on that column, despite the terminology "run-level port_outputs".
- `ActivityOutputArtifact` and `ActivityInputArtifact` do not carry `graph_instance_id` directly; they are scoped by containing `ActivityLifecycleRunState.graph_instance_id`. This is acceptable internally but ambiguous for aggregated API/read-model surfaces.
- I did not perform external research; no third-party references were needed.
- I did not run tests or modify implementation code, per research sub-agent scope.
