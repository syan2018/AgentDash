# Workflow Architecture

## Role

Workflow 子系统表达可执行 graph definition、编排运行态和状态推进规则。`WorkflowGraph` 是静态 workflow 定义输入；`OrchestrationPlanSnapshot` 是进入 runtime 前的不可变计划；`AgentProcedure` 表示单个 Agent Activity 的行为/能力/上下文/hook 契约。

## Core Vocabulary

| 概念 | 语义 |
| --- | --- |
| `AgentProcedure` | 单个 Agent Activity 的 behavior / capability / context / hook / port 契约 |
| `WorkflowGraph` | 可执行 Activity DAG definition |
| `LifecycleRun` | tracked life process / control ledger |
| `LifecycleContext` | `LifecycleRun` 内的上下文快照，保存主 AgentRun、AgentRun refs、AgentFrame refs、权限和预算摘要 |
| `OrchestrationInstance` | `LifecycleRun` 内部 0..N 个编排状态容器，保存 plan snapshot、runtime node state、dispatch 摘要和 state exchange snapshot；`orchestration_id` 是运行实例身份 |
| `OrchestrationPlanSnapshot` | 静态 graph、workflow script 或 run artifact 编译后的不可变 runtime plan |
| `LifecycleRunTopology` | run 的控制面拓扑：`graphless` 表示普通 Agent runtime，`workflow_graph` 表示显式 Activity graph runtime |
| `RuntimeNodeState` | `orchestration_id + node_path + attempt` 定位的运行节点状态 |
| `NodeDispatchLease` | scheduler 对 runtime node 的 operational lease |
| `LifecycleAgent` | run-scoped Agent runtime identity |
| `AgentRuntimeRefs` | Agent runtime 控制面引用，统一携带 run / agent / frame，并通过 orchestration/node refs 表达显式 workflow binding |
| `AgentInvocation` | runtime node 到 `LifecycleAgent`、`AgentFrame` 与 executor trace 的执行证据 |
| `AgentFrame` | Agent runtime surface revision，承载 capability、context、VFS 与 MCP |
| `RuntimeSession` | connector delivery / trace evidence |
| `RuntimeSessionExecutionAnchor` | `RuntimeSession` 反查 run / agent / frame / orchestration node 的权威索引 |
| `LifecycleSubjectAssociation` | `SubjectRef` 到 whole run 或 agent 的业务归属关联 |

## Invariants

- `WorkflowGraph` 是 workflow 运行、编辑和观察的主模型。
- `LifecycleRun` 是 tracked life process / control ledger；同一 run 可以包含 0..N 个 `OrchestrationInstance`。
- `LifecycleRun.context`、`LifecycleRun.orchestrations`、`LifecycleRun.view_projection` 是 orchestration contract 的 owning aggregate 字段；command/service 通过 aggregate 写入这些字段，repository 只做整体持久化。
- `OrchestrationInstance.orchestration_id` 是唯一运行实例身份；definition source / asset provenance 只能作为 plan metadata 或审计信息，不参与 scheduler、terminal callback、trace anchor 的节点坐标。
- 静态 `WorkflowGraph`、workflow script 或 run artifact 进入 runtime 前先由 application 层 compiler 生成 `OrchestrationPlanSnapshot(plan_digest=sha256:...)`；compiler blocking diagnostics 发生在 run/orchestration 创建前。
- Runtime node key 必须包含 `orchestration_id + node_path + attempt`，避免同一 Lifecycle 内多个 orchestration 的节点冲突。
- durable state advancement 只能通过 orchestration runtime event / journal materialization 进入 `RuntimeNodeState`。
- Scheduler 负责 durable claim 和 executor 启动；executor 只通过 runtime node terminal event 把结果交还给 orchestration runtime。
- Function executor 即使立即完成，也必须产出 runtime node terminal event，而不是直接修改 run state。
- Agent node execution identity 使用 `AgentInvocation(lifecycle_run_id, orchestration_id, node_path, attempt, agent_run_id, frame_id)` 定位当前 work；RuntimeSession 只作为 terminal/runtime evidence。
- 通过 `RuntimeSession` 反查 Lifecycle node 时，必须使用 runtime trace anchor，再进入 `LifecycleRun -> OrchestrationInstance -> RuntimeNodeState` 的证据链。
- `LifecycleSubjectAssociation` 是 Task / Story / Routine / Project 等业务 subject 的归属入口；业务状态不能由 `RuntimeSession` title、存在性或 trace 内容推断。
- Lifecycle edge 只有 `flow` 和 `artifact` 两类；artifact edge 隐含 node-level flow dependency。
- 多 activity lifecycle 必须显式声明 edges；运行时不按数组顺序推断推进路径。
- `workflow/value_objects.rs` 是可序列化 Workflow value types 的 facade；具体类型按 binding、contract、capability、mount directive、hook rule、ports、lifecycle definition、activity definition、run state 子模块组织。`workflow/validation.rs` 承载 definition、topology 与 activity lifecycle 校验。类型定义和校验分离，原因是持久化契约与规则演进有不同的变化节奏。

## Current Baseline

| 文档 | 当前职责 |
| --- | --- |
| `activity-lifecycle.md` | Activity executor、run startup、template install/update、drop-step migration 契约 |
| `lifecycle-edge.md` | DAG edge kind、校验、运行时推进规则 |
| `lifecycle-run-link.md` | LifecycleSubjectAssociation 关联层、Session 降级、subject/agent/run-oriented API 契约 |
| `../story-task-runtime.md` | Story / Task / Session / LifecycleRun 关系拓扑 |
| `../../frontend/workflow-activity-lifecycle.md` | 前端 Activity lifecycle 编辑与运行观察契约 |

`LifecycleRun.topology=graphless` 是普通 Agent runtime 的当前默认形态，只创建 run / agent / frame / runtime session anchor 与 subject association。`LifecycleRun.topology=workflow_graph` 表示显式 workflow runtime，拥有 root definition refs 与 `OrchestrationInstance`。

跨业务 dispatch / cancel / routine response 使用 `AgentRuntimeRefs` 作为统一控制面 envelope。显式 workflow runtime 的节点绑定使用 orchestration/node refs，原因是同一 Lifecycle 内可以同时存在多个 orchestration。

## Module Boundaries

| 模块 | 当前职责 |
| --- | --- |
| `workflow/value_objects.rs` | public facade 与 value object 测试入口 |
| `workflow/value_objects/binding.rs` | Workflow binding scope 类型与 owner 映射 |
| `workflow/value_objects/contract.rs` | Workflow contract、session terminal state、effective session contract |
| `workflow/value_objects/capability.rs` | CapabilityConfig、tool capability path / directive / reduction |
| `workflow/value_objects/mount_directive.rs` | VFS mount capability directive wire types |
| `workflow/value_objects/hook_rule.rs` | Workflow hook trigger 与 rule spec |
| `workflow/value_objects/ports.rs` | input/output port、gate/context strategy、standalone fulfillment |
| `workflow/value_objects/lifecycle_def.rs` | 当前 ActivityLifecycleDefinition 迁移来源；目标语义是 WorkflowGraph |
| `workflow/value_objects/activity_def.rs` | Activity definition、executor、completion/iteration/join/transition policy |
| `workflow/value_objects/run_state.rs` | Lifecycle execution ledger 与 executor refs |
| `workflow/value_objects/orchestration.rs` | Lifecycle-owned orchestration contract、plan snapshot、runtime node state、dispatch/state exchange/journal fact value types |
| `workflow/validation.rs` | Workflow contract validation、Lifecycle DAG validation、Activity lifecycle transition/port/policy validation |
| `agentdash-spi::workflow::script` | Workflow script evaluator port；application 只消费脚本校验与 builder document 输出，不依赖具体脚本引擎 |
| `agentdash-infrastructure::workflow_scripts` | Rhai workflow builder adapter；只注册 workflow helper surface，并复用公共 `RhaiScriptRuntime` |
| `agentdash-application::workflow::script` | typed workflow script builder document、preflight service 与 pathful diagnostics |
| `agentdash-application::workflow::orchestration::ScriptCompiler` | workflow script builder document -> `OrchestrationPlanSnapshot` compiler frontend |

## Local Decisions

- 普通 Agent runtime 默认使用 graphless 拓扑，原因是多数 Agent 会话只需要控制面、runtime trace 与 subject 归属；Activity graph 只在需要节点流转、attempt state、claim 和 assignment 的显式 workflow 中引入。
- 业务 result 不平铺 run / graph / agent / frame / node refs，原因是 run / agent / frame 是通用控制面，而 orchestration/node refs 是显式 workflow binding；统一 envelope 可以让调用方先处理 Agent runtime，再按拓扑决定是否进入 workflow 节点细节。
- artifact edge 自动提供 flow dependency，原因是数据依赖本身已经表达执行顺序，重复 flow edge 会制造两套 dependency 事实。
- `RuntimeSessionExecutionAnchor` 是 runtime trace/delivery refs 的索引和 read model projection 来源，原因是运行时 trace 反查需要稳定索引，且不应随 frame revision surface 变化。

## Scenario: Lifecycle Orchestration Contract

### 1. Scope / Trigger

- Trigger: 为 `LifecycleRun` 增加或消费 `context`、`orchestrations`、`view_projection` 字段，或修改 `OrchestrationInstance` / `OrchestrationPlanSnapshot` / `RuntimeNodeState` 等合同。
- Scope: domain value objects、`LifecycleRun` aggregate、`LifecycleRunRepository`、workflow runtime 事实源边界。

### 2. Signatures

Domain aggregate:

```rust
pub struct LifecycleRun {
    pub context: LifecycleContext,
    pub orchestrations: Vec<OrchestrationInstance>,
    pub view_projection: Option<serde_json::Value>,
}
```

Aggregate methods:

```rust
set_lifecycle_context(context: LifecycleContext)
add_orchestration(orchestration: OrchestrationInstance) -> bool
replace_orchestration(orchestration: OrchestrationInstance) -> Option<OrchestrationInstance>
orchestration_by_id(orchestration_id: Uuid) -> Option<&OrchestrationInstance>
```

PostgreSQL columns on `lifecycle_runs`:

```sql
context text DEFAULT '{}'::text NOT NULL
orchestrations text DEFAULT '[]'::text NOT NULL
view_projection text
```

### 3. Contracts

- `context` 保存 Lifecycle 级上下文引用，不内嵌完整 AgentFrame surface。
- `orchestrations` 保存同一 Lifecycle 内 0..N 个内部编排实例；`orchestration_id` 是运行实例身份。definition source、asset revision、script digest 等 provenance 可以保存在 plan metadata 或可选审计字段中，但不替代 `orchestration_id`。
- `OrchestrationPlanSnapshot.plan_digest` 是 compiled plan 内容身份；runtime、journal 和 projection 按 digest 判断 plan 合同，不使用 graph instance UUID 作为 plan 身份。
- graph-backed dispatch 创建 `OrchestrationInstance` 时，直接 materialize entry ready nodes、dispatch ready queue 和空 `StateExchangeSnapshot`。
- `view_projection` 是 read projection 占位，command/service 不从该字段反向推导 runtime state。
- Scheduler、terminal callback 与 projection 消费 orchestration runtime node state。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `LifecycleRun` constructor 创建 graphless / workflow_graph run | `context={}`、`orchestrations=[]`、`view_projection=None` |
| `add_orchestration` 收到重复 `orchestration_id` | 返回 `false`，不修改 aggregate |
| `replace_orchestration` 找不到 `orchestration_id` | 返回 `None` |
| repository 读取无效 `orchestrations` JSON 文本 | 返回带 `lifecycle_runs.orchestrations` 上下文的 `DomainError` |
| workflow asset compiler 产生 blocking diagnostics | 返回 `BadRequest`，且不创建 run / orchestration |
| service 创建 workflow runtime | 直接生成 `OrchestrationInstance`，entry semantic node 处于 `Ready` |

### 5. Good/Base/Bad Cases

- Good: `LifecycleRunRepository` create/update/select 对 `context`、`orchestrations`、`view_projection` 做整体 roundtrip。
- Base: graphless run 没有 orchestration instance，但仍能保存空 context 和空数组。
- Good: 同一 run 内 root workflow、append workflow、review flow 或 dynamic script 分别拥有独立 `orchestration_id`，从而共享 Lifecycle 容器但隔离 runtime node state。

### 6. Tests Required

- Domain serde roundtrip 覆盖 `OrchestrationPlanSnapshot`、`RuntimeNodeState`、`OrchestrationJournalFact`。
- `LifecycleRun` aggregate 测试覆盖 0、1、多个 `OrchestrationInstance`。
- Repository row parsing 测试覆盖新列默认值、坏 JSON 错误上下文。
- PostgreSQL repository roundtrip 测试覆盖 create -> get -> update -> get。
- Application compiler/runtime 测试覆盖静态 graph plan digest、semantic node kind、artifact state exchange、blocking diagnostics 与 entry ready node materialization。
- Dispatch service 测试覆盖 start lifecycle run、普通 workflow dispatch、append workflow 和 compiler preflight failure，且断言运行实例身份来自 `orchestration_id`。
- 触及 migration 时运行 `pnpm run migration:guard`。

### 7. Current Runtime Entry

正式 runtime 接入点是 orchestration activation：compiler 输出不可变 plan，runtime helper 直接 materialize initial `OrchestrationInstance`。后续 scheduler 继续消费同一 `OrchestrationPlanSnapshot` 和 `RuntimeNodeState` 合同推进节点、state exchange 与 terminal materialization。

## Scenario: Orchestration Runtime Reducer

### 1. Scope / Trigger

- Trigger: application 层推进 `OrchestrationInstance` 中的 runtime node 状态，或把 `complete_lifecycle_node` / session terminal callback 接到 common runtime。
- Scope: `workflow/orchestration/runtime.rs`、`LifecycleOrchestrator`、runtime node output materialization、state exchange、ready queue。

### 2. Signatures

Application reducer events:

```rust
pub enum OrchestrationRuntimeEvent {
    NodeStarted {
        node_path: String,
        attempt: u32,
        executor_run_ref: Option<ExecutorRunRef>,
        timestamp: DateTime<Utc>,
    },
    NodeCompleted {
        node_path: String,
        attempt: u32,
        outputs: Vec<NodePortValue>,
        timestamp: DateTime<Utc>,
    },
    NodeFailed {
        node_path: String,
        attempt: u32,
        error: RuntimeNodeError,
        timestamp: DateTime<Utc>,
    },
    NodeCancelled {
        node_path: String,
        attempt: u32,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
}
```

Reducer entry:

```rust
apply_orchestration_event(
    instance: &mut OrchestrationInstance,
    event: OrchestrationRuntimeEvent,
) -> Result<OrchestrationRuntimeApplyOutcome, OrchestrationRuntimeError>

apply_orchestration_event_to_run(
    run: LifecycleRun,
    orchestration_id: Uuid,
    event: OrchestrationRuntimeEvent,
) -> Result<(LifecycleRun, OrchestrationRuntimeApplyOutcome), OrchestrationRuntimeError>
```

### 3. Contracts

- Reducer 只消费 `OrchestrationInstance` / plan rules / event；外部仓储只负责读取和写回 `LifecycleRun` aggregate。
- `NodeStarted` 将 matching `RuntimeNodeState` 置为 `Running`，写入 `executor_run_ref`，并从 `ExecutorRunRef` 派生 `RuntimeTraceRef` 去重追加到 `trace_refs`。
- `NodeCompleted` 校验 completion policy、写 node `outputs`、写 `StateExchangeSnapshot.node_outputs`，再按 `StateExchangeRule` 物化 successor inputs。
- transition activation 只把满足 condition / join policy 的 Pending successor 置为 `Ready`，并同步 `activation.ready_node_ids` 与 `dispatch.ready_node_ids`。
- condition false 且所有 incoming source terminal 时，successor 置为 `Skipped`，避免保留不可解释的 Pending node。
- 本阶段尚未执行的 `max_traversals` 以 blocking diagnostic 置目标 node 为 `Blocked`，错误写入 `RuntimeNodeError`；不能静默激活。
- terminal event 对已经 `Completed` / `Failed` / `Cancelled` / `Skipped` 的 node 幂等，不重复物化 state exchange 或激活后继。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| event 指向不存在的 `node_path + attempt` | `NodeNotFound` |
| plan node 缺失 | `PlanNodeNotFound` |
| completion policy 要求的 output port 未提交 | `CompletionPolicyRejected`，`complete_lifecycle_node` 返回 gate rejected |
| state exchange source output 缺失 | `StateExchangeMissingOutput`，`complete_lifecycle_node` 返回 gate rejected |
| transition condition 不满足且 incoming sources terminal | successor `Skipped` |
| transition 带 `max_traversals` | successor `Blocked` + `max_traversals_not_supported` diagnostic |
| duplicate terminal event | `terminal_idempotent=true`，不改 outputs，不重复 ready successor |

### 5. Good/Base/Bad Cases

- Good: entry node completed with output `{ proposal: ... }` materializes `state_snapshot.node_outputs["entry"]["proposal"]` and successor input port.
- Base: simple `Always` transition activates one successor and updates ready queue.
- Bad: missing required output port keeps current node unmodified and reports gate rejection to the tool caller.

### 6. Tests Required

- Unit: activation materializes entry ready nodes.
- Unit: `NodeStarted` writes executor ref, trace ref, and clears ready queue.
- Unit: `NodeCompleted` activates simple transition.
- Unit: state exchange copies output into successor input.
- Unit: duplicate terminal event is idempotent.
- Unit: condition false skips unreachable successor.
- Unit: `max_traversals` blocks successor with diagnostic.
- Integration: session terminal callback and `complete_lifecycle_node` route through runtime node resolver and reducer.

### 7. Wrong vs Correct

#### Wrong

```text
RuntimeSession terminal -> mutate RuntimeNodeState directly in orchestrator
```

#### Correct

```text
RuntimeSession terminal -> RuntimeTraceAnchor -> OrchestrationRuntimeEvent -> reducer -> LifecycleRun.orchestrations[]
```

## Scenario: Graph-backed AgentCall Dispatch Start

### 1. Scope / Trigger

- Trigger: `LifecycleDispatchService::dispatch_common` 已为 graph-backed dispatch 创建 runtime session、AgentFrame、current agent frame 和 `RuntimeSessionExecutionAnchor`。
- Scope: graph-backed entry `AgentCall` 的 launch evidence materialization；不包含完整 scheduler / executor launcher。

### 2. Signatures

Dispatch path must submit:

```rust
OrchestrationRuntimeEvent::NodeStarted {
    node_path: orchestration_binding.node_path,
    attempt: orchestration_binding.attempt,
    executor_run_ref: Some(ExecutorRunRef::RuntimeSession { session_id }),
    timestamp,
}
```

### 3. Contracts

- Graph-backed dispatch 创建 runtime session 后，entry runtime node 必须通过 reducer 从 `Ready` 转为 `Running`。
- Reducer 写入 `executor_run_ref` 和 `RuntimeTraceRef::RuntimeSession`，并移除 ready queue 中的 entry node。
- 更新后的 `LifecycleRun` 必须回写 repository，且 dispatch result 的 `runtime_refs` 仍使用同一个 `orchestration_id + node_path + attempt`。
- `start_lifecycle_run` 只初始化 orchestration，不创建 runtime session，因此 entry node 仍为 `Ready`。
- 如果 graph-backed dispatch 无法取得 runtime session id，不能伪造 `NodeStarted`；应返回内部错误并保持事实一致。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| graph-backed dispatch 成功创建 runtime session | entry node `Running`，ready queues 为空 |
| `RuntimePolicy` 未提供 session id | 返回 internal error，不写 fake executor ref |
| `start_lifecycle_run` | entry node `Ready`，无 executor ref |

### 5. Good/Base/Bad Cases

- Good: subject execution with workflow graph creates one runtime session anchor and the root node has matching `ExecutorRunRef::RuntimeSession` plus trace ref.
- Base: lifecycle start API creates orchestration only; no agent/frame/session side effect.
- Bad: dispatch returns a delivery runtime session while `RuntimeNodeState` stays `Ready` with no executor ref.

### 6. Tests Required

- Dispatch service graph-backed subject execution asserts node `Running`、empty ready queues、executor ref、trace ref、anchor refs。
- Project agent graph dispatch by key asserts the same started materialization。
- Lifecycle run start test asserts entry remains `Ready`。

### 7. Wrong vs Correct

#### Wrong

```text
dispatch_common creates RuntimeSessionExecutionAnchor, but leaves node Ready
```

#### Correct

```text
dispatch_common creates anchor -> submits NodeStarted -> persists LifecycleRun with node Running
```

## Scenario: Workflow Script Builder Frontend

### 1. Scope / Trigger

- Trigger: 新增或修改 workflow script builder helper、`WorkflowScriptEvaluator` SPI、typed builder document、preflight API，或 workflow script 到 OrchestrationPlan compiler 的映射。
- Scope: `agentdash-spi::workflow::script`、`agentdash-infrastructure::workflow_scripts`、`agentdash-application::workflow::script`、`agentdash-application::workflow::orchestration::ScriptCompiler`、`/api/workflow-scripts/preflight`。

### 2. Signatures

SPI port:

```rust
pub trait WorkflowScriptEvaluator: Send + Sync {
    fn validate_workflow_script(&self, script: &str) -> Result<(), Vec<String>>;
    fn eval_workflow_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}
```

Application builder parse:

```rust
parse_workflow_script_builder_document(
    value: serde_json::Value,
) -> WorkflowScriptBuilderParseOutput
```

Application preflight:

```rust
WorkflowScriptPreflightService::preflight(
    WorkflowScriptPreflightInput {
        evaluator: &dyn WorkflowScriptEvaluator,
        compiler: &dyn WorkflowScriptCompiler,
        source_text,
        ctx,
        args,
        source_ref,
        provenance,
    },
) -> WorkflowScriptPreflightOutput
```

首批 builder statement:

```rust
WorkflowScriptStatement::{
    Phase,
    Log,
    Agent,
    Parallel,
    Pipeline,
    Function,
    LocalEffect,
    HumanGate,
}
```

### 3. Contracts

- Rhai workflow evaluator 只能返回 serializable builder document；不能启动 AgentRun、FunctionRun、LocalEffect，不能访问文件系统、shell 或网络。
- Hook Rhai helper 与 workflow Rhai helper 分属不同 adapter；workflow evaluator 不注册 `block` / `inject` / `approve` 等 Hook helper。
- Rhai helper 输出必须包含 `kind` discriminator；reserved fields such as `kind` / `name` / `request` / `effect` 由 helper 拥有，脚本 options 不能覆盖。
- Application 层先把 evaluator 返回的 JSON 解析为 typed builder document，再进入 `ScriptCompiler`；`serde_json::Value` 不作为长期 compiler 输入。
- typed builder parser 负责结构合同和 source-path diagnostics；变量解析、唯一命名、capability 声明、预算/并发上限和 plan mapping 由 `ScriptCompiler` / preflight 负责。
- `ScriptCompiler` 输出 `OrchestrationPlanSnapshot`，并把 root args binding、log marker、capability summary 和 source provenance 放入 plan metadata，runtime activation 只消费 plan 合同。
- `POST /api/workflow-scripts/preflight` 是审批前无副作用入口；它只返回 source digest、source ref、builder document、diagnostics、plan preview、plan snapshot 和 capability summary，不创建 `LifecycleRun` 或 `OrchestrationInstance`。
- `RhaiWorkflowScriptEvaluator` 位于 infrastructure，原因是 Rhai engine、sandbox 与 helper 注册是具体技术 adapter；application 只依赖 SPI port 与 typed builder contract。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| Rhai syntax invalid | `validate_workflow_script` 返回 diagnostic strings |
| workflow evaluator 调用 Hook helper | eval 返回 Rhai unknown function error |
| builder document root `kind != workflow` | `unexpected_builder_document_kind` |
| statement `kind` 不在首批 primitive 内 | `unknown_primitive`，source path 指向 `<path>.kind` |
| primitive 缺少必填字段 | `missing_field`，source path 指向缺失字段 |
| string / array 字段类型错误 | `invalid_field_type`，source path 指向错误字段或元素 |
| request / effect helper kind 未知 | `unknown_request_helper` / `unknown_effect_helper` |

### 5. Good/Base/Bad Cases

- Good: `workflow(#{ body: [phase(\"collect\", [parallel([...])])] })` eval 后解析为 typed `WorkflowScriptBuilderDocument`。
- Base: `agent` 的 `inputs` / `outputs` 缺省为空数组，compiler 在需要 state exchange 或 root args binding 时再决定是否产生 blocking diagnostic。
- Bad: Rhai helper 直接执行 HTTP、shell 或文件写入；这些副作用必须只在 common orchestration runtime executor 阶段发生。

### 6. Tests Required

- SPI crate 编译通过，保证 port 不依赖 application / infrastructure。
- Infrastructure test 覆盖 workflow evaluator eval builder document、syntax validate、Hook helper unavailable。
- Application test 覆盖 phase / parallel / agent / pipeline / function / local_effect / human_gate 的 typed parse。
- Application test 覆盖 pathful diagnostics，至少断言每类 primitive 的缺失字段 path。
- Application compiler test 覆盖 phase/pipeline/parallel/function/local_effect/human_gate/state exchange/root args/materialized activation/deterministic digest。
- API/contracts check 覆盖 preflight DTO 导出与 `cargo check -p agentdash-api`。
- Hook test 回归，保证公共 Rhai runtime 抽取后 Hook helper surface 未变化。

### 7. Wrong vs Correct

#### Wrong

```text
RhaiWorkflowScriptEvaluator -> OrchestrationPlanSnapshot
```

#### Correct

```text
RhaiWorkflowScriptEvaluator -> builder document JSON
  -> agentdash-application::workflow::script typed builder document
  -> ScriptCompiler -> OrchestrationPlanSnapshot
```

## Contract Appendices

- [Activity Lifecycle Backend Contract](./activity-lifecycle.md)
- [Lifecycle Edge](./lifecycle-edge.md)
- [Story / Task Runtime](../story-task-runtime.md)
- [Activity Lifecycle Frontend Contract](../../frontend/workflow-activity-lifecycle.md)
