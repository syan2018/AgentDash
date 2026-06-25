# Research: hooks crate split boundary

- Query: review AgentDash backend `hooks` module crate split boundary and produce executable split recommendations
- Scope: internal
- Date: 2026-06-25

## Findings

### Conclusion

推荐新建 `agentdash-application-hooks`，但不要把当前 `crates/agentdash-application/src/hooks/**` 原样搬过去。当前 hooks 目录混合了三类职责：

1. Hook policy engine：rule merge、builtin/global rules、contract-driven preset/script evaluation、preset registry。
2. Hook provider facade：实现 `agentdash_spi::hooks::ExecutionHookProvider`，把已解析的 active workflow / owner facts 转成 `AgentFrameHookSnapshot` 并评估 `HookResolution`。
3. Lifecycle/query bridge：从 `HookControlTarget` / runtime session 反查 `ActiveWorkflowProjection`、`LifecycleSubjectAssociation`、scoped port outputs、execution log。

正确边界是：`agentdash-application-hooks` 拥有 1 和 provider facade 的 policy 组装部分；Lifecycle/query bridge 不应作为 repository-heavy 实现留在 hooks crate，应改为 `agentdash-application-ports` 中的 hook-specific projection/effect port，由 lifecycle/workflow implementation 实现。

### Files found

- `crates/agentdash-application/src/hooks/mod.rs` — 当前 hooks facade，公开 `ActiveWorkflowSnapshotBuilder`、`SessionOwnerResolver`、preset registry、provider。
- `crates/agentdash-application/src/hooks/provider.rs` — 当前 `AppExecutionHookProvider` facade，同时持有 repos、workflow snapshot builder、owner resolver、script engine，并实现 `ExecutionHookProvider`。
- `crates/agentdash-application/src/hooks/rules.rs` — rule merge 入口，依次应用 global rules、active workflow contract rules、owner default rules。
- `crates/agentdash-application/src/hooks/presets.rs` — builtin preset registry，`include_str!` 读取 `.rhai` preset scripts。
- `crates/agentdash-application/src/hooks/script_engine.rs` — application-side script facade，构造 ctx JSON、调用 SPI `HookScriptEvaluator`、解析 `ScriptDecision`。
- `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs` — 当前 lifecycle projection bridge，直接依赖 lifecycle projection resolver 和 execution log flush。
- `crates/agentdash-application/src/hooks/active_workflow_contribution.rs` — active workflow step/guidance markdown injection renderer。
- `crates/agentdash-application/src/hooks/fragment_bridge.rs` — `HookInjection -> ContextFragment` bridge，但 runtime-session 已有同类内部实现。
- `crates/agentdash-application-runtime-session/src/hooks.rs` — runtime-session 内部 `HookInjection -> ContextFragment` bridge。
- `crates/agentdash-application-runtime-session/src/session/hooks_service.rs` — session runtime 以 `Arc<dyn ExecutionHookProvider>` 加载 snapshot，并通过 hook target port 创建 `SharedHookRuntime`。
- `crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs` — `AgentFrameHookRuntime`，缓存 snapshot/diagnostics/trace/pending actions，并通过 provider 执行 refresh/evaluate。
- `crates/agentdash-api/src/bootstrap/session.rs` — composition root，构造 concrete `AppExecutionHookProvider` 并注入 runtime-session builder。
- `crates/agentdash-api/src/routes/workflows.rs` — hook preset/script HTTP endpoints。
- `crates/agentdash-spi/src/hooks/mod.rs` — hook SPI contract：snapshot、runtime access、provider trait、resolution。
- `crates/agentdash-spi/src/hooks/script.rs` — `HookScriptEvaluator` port。
- `crates/agentdash-application-ports/src/lifecycle_surface_projection.rs` — 已存在 lifecycle surface projection types，可作为 hook projection port 的落点。
- `.trellis/spec/backend/hooks/architecture.md` — hooks 架构规范。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` — execution hook runtime 跨层契约。
- `.trellis/spec/backend/hooks/hook-script-engine.md` — Rhai hook script engine 契约。

### Code patterns

- 当前 `agentdash-application::hooks` 对外公开面过宽：`mod.rs` 公开 `ActiveWorkflowSnapshotBuilder`、`SessionOwnerResolver`、`HookRulePreset`、`AppExecutionHookProvider`（`crates/agentdash-application/src/hooks/mod.rs:16`-`20`）。拆分后 public API 应只保留 provider factory / preset registry / script validation service。
- `AppExecutionHookProvider` 当前直接持有 `InlineFileRepository`、`SessionOwnerResolver`、`ActiveWorkflowSnapshotBuilder`、`HookScriptEngine`（`crates/agentdash-application/src/hooks/provider.rs:34`-`39`），其 deps struct 直接收 project/story/lifecycle/frame/anchor/inline repos（`provider.rs:41`-`51`）。这是 crate split 的主要耦合点。
- Provider snapshot assembly 直接构造 `ActiveWorkflowMeta.effective_contract`（`provider.rs:192`-`199`）、读取 scoped port outputs（`provider.rs:218`-`230`）、渲染 workflow injections（`provider.rs:255`）。这些应该由 hook projection port 提供闭包 facts，hooks 只负责转成 SPI snapshot。
- Provider 实现 `ExecutionHookProvider` 的正式入口是 `load_frame_snapshot`、`refresh_frame_snapshot`、`evaluate_frame_hook`、`append_execution_log`（`provider.rs:264`-`314`）。这是新 crate 的核心 public behavior。
- Rule engine 已经是可搬迁边界：global rules 和 owner defaults 是 private modules（`rules.rs:14`-`16`），`apply_hook_rules` 是内部入口（`rules.rs:93`），脚本引擎通过 `HookScriptEvaluator` port 注入（`script_engine.rs:47`-`52`）。
- Script engine 的 ctx 和 decision parsing 是 hooks policy surface：`build_ctx_value`（`script_engine.rs:105`）和 `parse_decision`（`script_engine.rs:184`）不需要依赖 session/runtime/lifecycle implementation。
- Preset registry 是 hooks crate public API：`HookRulePreset`（`presets.rs:15`）、`hook_rule_preset_registry`（`presets.rs:132`）、`builtin_preset_scripts`（`presets.rs:137`）、domain trigger mapping（`presets.rs:141`）。
- Active workflow bridge 当前直接调用 lifecycle resolver / execution log：`resolve_active_workflow_for_target`（`active_workflow_snapshot.rs:73`）、`append_execution_log`（`active_workflow_snapshot.rs:87`）。这些不应作为 hooks crate 的 repository dependency。
- Runtime-session 已经只要求 trait object：`SessionRuntimeBuilder::new_with_hooks_and_persistence` 接收 `Option<Arc<dyn ExecutionHookProvider>>`（`crates/agentdash-application-runtime-session/src/session/runtime_builder.rs:36`-`39`）；hub factory 同样保存 trait object（`session/hub/factory.rs:94`-`105`）。所以 runtime-session 不需要依赖 new hooks crate。
- AgentRun hook runtime 也只持有 `Arc<dyn ExecutionHookProvider>`（`crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:40`-`47`），实现 `HookRuntimeAccess`（`hook_runtime.rs:231`）。所以 AgentRun 不需要依赖 new hooks crate。
- `RuntimeSessionHookTargetRuntimeRequest` 在 ports 中携带 provider trait object 和 snapshot（`crates/agentdash-application-ports/src/runtime_session_live.rs:56`-`61`），`RuntimeSessionHookTargetPort` 创建 `SharedHookRuntime`（`runtime_session_live.rs:65`-`68`）。这个方向已经正确。
- API 目前泄露 concrete provider：bootstrap output 和 AppState 都是 `Arc<AppExecutionHookProvider>`（`crates/agentdash-api/src/bootstrap/session.rs:169`、`crates/agentdash-api/src/app_state.rs:114`），workflow route 直接调用 concrete `validate_script/register_preset/remove_preset`（`crates/agentdash-api/src/routes/workflows.rs:1215`-`1250`）。拆分时 API 应依赖 hooks crate public service，而 session runtime 继续只拿 SPI trait object。
- Hook endpoints 位于 workflows route：`/hook-presets`、`/hook-scripts/validate`、`/hook-presets/custom`、`/hook-presets/custom/{key}`（`crates/agentdash-api/src/routes/workflows.rs:122`-`133`）。短期可保留 route 位置，导入改为新 crate。
- `agentdash-application-ports` 已有 lifecycle projection DTO：`ActiveWorkflowProjection`（`crates/agentdash-application-ports/src/lifecycle_surface_projection.rs:30`）和 `RuntimeNodeArtifactScope`（`lifecycle_surface_projection.rs:185`），也已有 `LifecycleSurfaceProjectionPort`（`lifecycle_surface_projection.rs:575`）。但当前 hooks 仍吃 lifecycle crate 自己的 `ActiveWorkflowProjection`（`crates/agentdash-application-lifecycle/src/lifecycle/projection.rs:25`）。

### Recommended crate boundary

#### New crate

Name: `agentdash-application-hooks`

Owns:

- `provider.rs`, after refactoring deps from repository set to ports.
- `rules.rs`
- `rules/global_rules/**`
- `rules/owner_defaults/**`, only if owner defaults consume already-projected `SubjectRunContext` / snapshot facts. If they need repositories, move that lookup behind the projection port first.
- `script_engine.rs`
- `presets.rs`
- `helpers.rs`
- `snapshot_helpers.rs`
- `active_workflow_contribution.rs`, after replacing direct `ActiveWorkflowProjection` dependency with a hook projection facts type or ports-level projection type.
- `test_fixtures.rs` under test module or `test-support`, not public API.
- `scripts/hook-presets/*.rhai` moved under the new crate, because `presets.rs` uses compile-time `include_str!` and these scripts are part of hooks policy assets.

Does not own:

- `fragment_bridge.rs` from `agentdash-application/src/hooks`; mapping `HookInjection -> ContextFragment/Contribution` belongs where `Contribution` is defined. Runtime-session already has `crates/agentdash-application-runtime-session/src/hooks.rs` for this.
- `ActiveWorkflowSnapshotBuilder` in current form. It is lifecycle/query bridge, not hook policy. Replace it with a port dependency.
- `SessionOwnerResolver` in current form. It resolves lifecycle subject ownership via project/story/lifecycle repos; move this behind lifecycle projection / subject context port.
- `AgentFrameHookRuntime`; it belongs to `agentdash-application-agentrun` because it is an AgentFrame runtime cache/adaptor, not a policy provider.
- `HookRuntimeDelegate`; it belongs to `agentdash-application-runtime-session` because it adapts hook runtime to `AgentRuntimeDelegate` and turn lifecycle.
- `RhaiHookScriptEvaluator`; it stays in infrastructure because it is a concrete script runtime adapter implementing `agentdash_spi::HookScriptEvaluator`.
- API route DTO / handlers; routes remain in `agentdash-api`, importing the new hooks crate.

#### Public API surface

Keep small:

- `AppExecutionHookProvider` or rename to `ApplicationHookProvider`.
- `ApplicationHookProviderDeps`, with ports rather than repositories.
- `impl ExecutionHookProvider for ApplicationHookProvider`.
- `HookRulePreset`, `PresetSource`, `hook_rule_preset_registry`.
- A concrete admin/script surface used by API, e.g. `HookScriptAdmin { validate_script, register_preset, remove_preset }`; this can be methods on provider if AppState keeps a hooks service handle.

Do not expose:

- `HookScriptEngine`
- `ScriptDecision`
- `apply_hook_rules`
- `ActiveWorkflowSnapshotBuilder`
- `SessionOwnerResolver`
- `snapshot_helpers`

### Dependency direction

Final desired dependencies:

```text
agentdash-api
  -> agentdash-application-hooks
  -> agentdash-application-ports
  -> agentdash-domain
  -> agentdash-spi

agentdash-api
  -> agentdash-application-runtime-session
  -> agentdash-spi

agentdash-api
  -> agentdash-application-agentrun
  -> agentdash-spi / agentdash-application-ports
```

`agentdash-application-hooks` should depend on:

- `agentdash-spi`: yes. `ExecutionHookProvider`, `HookScriptEvaluator`, snapshot/resolution/runtime hook types are SPI contracts.
- `agentdash-domain`: yes for current workflow hook rule/contract types, unless those are later moved to SPI/contracts. This is already true because `agentdash-spi::hooks::ActiveWorkflowMeta` itself references `EffectiveSessionContract` and `LifecycleRunStatus`.
- `agentdash-application-ports`: yes. Add a hook-specific lifecycle projection/effect port here.
- `agentdash-application-lifecycle`: no in final state. Lifecycle should implement the projection/effect port; hooks should consume the port.
- `agentdash-application-runtime-session`: no.
- `agentdash-application-agentrun`: no.
- `agentdash-application`: no.
- `agentdash-infrastructure`: no production dependency. API composition root injects `RhaiHookScriptEvaluator` as `HookScriptEvaluator`.

Dependencies to convert to ports:

- Active workflow resolution: current `resolve_active_workflow_projection_for_target` / `resolve_active_workflow_projection_from_message_stream_trace` should be consumed through a new hook projection port, not called by hooks directly.
- Subject run context: current `build_subject_run_context` and `SessionOwnerResolver` should be lifecycle projection output, not hooks repository lookup.
- Fulfilled output ports: current `load_scoped_port_output_map` should be projection output or a lifecycle artifact query port.
- Execution log flush: current `append_execution_log` path should be an effect port method. `ExecutionHookProvider::append_execution_log` can remain SPI-facing, but implementation should delegate to a port.
- Inline file repository: remove from hooks deps by moving port-output fulfillment lookup to lifecycle projection.

Suggested port shape in `agentdash-application-ports`:

```rust
pub struct HookWorkflowProjectionQuery {
    pub target: HookControlTarget,
    pub provenance: RuntimeAdapterProvenance,
}

pub struct HookActiveWorkflowProjection {
    pub source_key: String,
    pub source_label: String,
    pub run_context: Option<SubjectRunContext>,
    pub active_workflow_meta: ActiveWorkflowMeta,
    pub step_summary: HookStepSummary,
    pub guidance: Option<String>,
}

#[async_trait]
pub trait HookWorkflowProjectionPort: Send + Sync {
    async fn load_active_hook_projection(
        &self,
        query: HookWorkflowProjectionQuery,
    ) -> Result<Option<HookActiveWorkflowProjection>, HookProjectionError>;

    async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookProjectionError>;
}
```

The exact type names can vary, but the important rule is that hooks receives closed hook facts, not repositories.

### Workflow split ordering

Recommended sequence: do not wait for the full workflow crate split, but do add the hook-specific projection port before moving hooks.

Reasoning:

- Full workflow split first is cleanest if it is already scheduled immediately. Hooks would then consume a stable workflow/lifecycle projection port from the new workflow crate or ports crate.
- Hooks first is still feasible if the first step is the projection port. This gives a small, independently verifiable extraction and prevents hooks from depending on `agentdash-application-lifecycle` directly.
- Hooks first by directly depending on `agentdash-application-lifecycle` will compile faster but is the wrong final edge: it preserves the current `ActiveWorkflowSnapshotBuilder` coupling and forces the same boundary work to be redone when workflow is split.

Impact:

- Workflow-first impact: fewer hooks-specific rewrites later, but larger blast radius because workflow/lifecycle/application boundaries are already broad.
- Hooks-first-with-port impact: small blast radius, and creates a reusable contract that helps workflow split later.
- Hooks-first-with-direct-lifecycle-dep impact: lowest immediate effort, highest rework risk; not recommended.

### Migration steps

1. Add hook projection/effect port in `agentdash-application-ports`.
   - Move only contract types needed by hooks, not repository logic.
   - Include active workflow meta/source/guidance/output-port fulfillment/run context and append execution log.

2. Implement the port in lifecycle/application boundary.
   - Use existing logic from `active_workflow_snapshot.rs`, `owner_resolver.rs`, `lifecycle::projection`, `lifecycle::session_run_context_resolver`, and `lifecycle::execution_log`.
   - Stop returning raw repos to hooks.

3. Create `crates/agentdash-application-hooks`.
   - Move policy modules and preset scripts.
   - Refactor `AppExecutionHookProviderRepos` into `ApplicationHookProviderDeps { workflow_projection: Arc<dyn HookWorkflowProjectionPort>, script_evaluator: Arc<dyn HookScriptEvaluator> }`.
   - Keep `ExecutionHookProvider` implementation behavior unchanged.

4. Update workspace and imports.
   - Add workspace member/dependency.
   - Change `agentdash-api` bootstrap/imports from `agentdash_application::hooks::*` to `agentdash_application_hooks::*`.
   - Keep runtime-session and agentrun dependencies unchanged; they already use `ExecutionHookProvider` trait object.

5. Remove or stop exporting old `agentdash-application::hooks`.
   - Since the project is pre-release and does not require compatibility shims, prefer deleting the old module once callers are moved.
   - Keep no re-export facade in `agentdash-application`.

6. Fix tests and preset asset paths.
   - Move hooks unit tests into new crate.
   - Replace tests that import `agentdash_infrastructure::RhaiHookScriptEvaluator` with either a fake `HookScriptEvaluator` or a dev-dependency if integration coverage needs real Rhai.
   - Ensure `include_str!` paths for `.rhai` presets resolve inside the new crate.

### Risk files

- `crates/agentdash-application/src/hooks/provider.rs`: highest risk; combines provider facade, workflow projection assembly, port-output lookup and rule evaluation.
- `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs`: direct lifecycle dependency and execution log write.
- `crates/agentdash-application/src/hooks/owner_resolver.rs`: repository-heavy owner context lookup.
- `crates/agentdash-application/src/hooks/presets.rs` plus `crates/agentdash-application/scripts/hook-presets/*.rhai`: compile-time asset path risk.
- `crates/agentdash-api/src/bootstrap/session.rs`: provider construction and script evaluator injection.
- `crates/agentdash-api/src/app_state.rs`: concrete provider type stored in services.
- `crates/agentdash-api/src/routes/workflows.rs`: hook preset/script endpoints currently import `agentdash_application::hooks`.
- `crates/agentdash-application-runtime-session/src/session/hooks_service.rs`: should not require business changes, but is the main integration gate for snapshot load/rebuild.
- `crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs`: should remain unchanged except import/test fallout; validates runtime/provider trait boundary.
- `Cargo.toml` and affected crate `Cargo.toml` files: workspace dependency ordering and cycle risk.

### Validation commands

Focused checks after the split:

```bash
cargo check -p agentdash-application-hooks
cargo test -p agentdash-application-hooks hooks
cargo test -p agentdash-application-runtime-session hook
cargo test -p agentdash-application-agentrun hook_runtime
cargo check -p agentdash-api
```

Broader gates before completing the task:

```bash
cargo check --workspace
pnpm run contracts:check
```

No database migration is expected for this split. Run `pnpm run migration:guard` only if a later implementation unexpectedly changes persisted lifecycle/session schema.

### Related specs

- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/hooks/architecture.md`
- `.trellis/spec/backend/hooks/execution-hook-runtime.md`
- `.trellis/spec/backend/hooks/hook-script-engine.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/execution-context-frames.md`

### External references

- No external web references used. This review is based on repository code and Trellis specs.

## Caveats / Not Found

- `task.py current --source` returned no active session task, so this research used the task directory explicitly provided by the user: `.trellis/tasks/06-25-backend-large-module-crate-split-plan`.
- I did not modify business code and did not run validation commands, because the request is review/planning only.
- I did not inspect `shared_library` or unrelated workflow split details beyond hooks-adjacent lifecycle/workflow projection boundaries.
